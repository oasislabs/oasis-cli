use std::{
    borrow::Cow,
    cell::{Cell, UnsafeCell},
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Component, Path, PathBuf},
    pin::Pin,
};

use bitflags::bitflags;
use oasis_rpc::import::ImportLocation;

use crate::{
    cmd,
    errors::{Result, WorkspaceError},
};

pub struct Workspace {
    root: PathBuf,

    // *Note*: Unsafety allows a `Target`s to contain a reference to is containing `Project`.
    // This makes it possible to work directly with `&Target`s and not some extra structure.
    // The only requirement is that a `Target` is dropped before its containing `Project`
    // Pin<Box<ing>> the `Project` means that it's not moving--even if the `Vec` reallocates.
    // Invariant: `Projects` are never removed from the `Vec`.
    projects: UnsafeCell<Vec<Pin<Box<Project>>>>,
}

impl Workspace {
    pub fn populate() -> Result<Self> {
        let cwd = std::env::current_dir().unwrap(); // Checked during initialization.
        let repo_root = cwd
            .ancestors()
            .find(|a| a.join(".git").is_dir())
            .ok_or_else(|| WorkspaceError::NoWorkspace(cwd.display().to_string()))?;

        let mut walk_builder = ignore::WalkBuilder::new(repo_root);
        walk_builder.sort_by_file_path(|a, b| {
            match a.components().count().cmp(&b.components().count()) {
                std::cmp::Ordering::Equal => a.cmp(b),
                ord => ord,
            }
        });
        let manifest_walker = walk_builder.build().filter_map(|de| match de {
            Ok(de)
                if de.file_type().map(|ft| ft.is_file()).unwrap_or_default()
                    && (de.file_name() == "Cargo.toml" || de.file_name() == "package.json") =>
            {
                Some(de)
            }
            _ => None,
        });

        let mut projects = Vec::new();
        let mut seen_manifest_paths = BTreeSet::new();
        for manifest_de in manifest_walker {
            for proj in Self::load_projects_from_manifest(manifest_de.path())? {
                if !seen_manifest_paths.contains(&proj.manifest_path) {
                    seen_manifest_paths.insert(proj.manifest_path.to_path_buf());
                    projects.push(proj);
                }
            }
        }

        debug!("detected workspace containing: {:?}", projects);

        Ok(Self {
            root: repo_root.to_path_buf(),
            projects: UnsafeCell::new(projects),
        })
    }

    /// Collects the set of top-level dependencies that are matched by the input `target_strs`.
    /// A valid target str is either the name of a service or a path in the workspace that
    /// points to a directory that contains services. Like git, `:/` refers to the workspace root.
    pub fn collect_targets<'a, 't>(
        &'a self,
        target_strs: &'t [&'t str],
    ) -> Result<Vec<&'a Target>> {
        let cwd = std::env::current_dir()?;
        let target_strs = if target_strs.is_empty() {
            Cow::Owned(vec![cwd.to_str().unwrap()])
        } else {
            Cow::Borrowed(target_strs)
        };
        TopTargets::new(self, &target_strs).collect_targets()
    }

    /// Returns the input targets and their dependencies in topologically sorted order.
    /// Returns an error if a dependency is missing or cyclic.
    pub fn construct_build_plan<'a>(
        &'a self,
        top_targets: &[&'a Target],
    ) -> Result<Vec<&'a Target>> {
        let mut build_plan: Vec<&Target> = Vec::new();
        let mut top_deps: Vec<Vec<&Target>> = Vec::new();
        for top_target in top_targets {
            let dep_targets = self.dependencies_of(top_target)?;

            // Propagate top target artifact requirements.
            for dep_target in dep_targets.iter() {
                dep_target.artifacts.update(|default_artifacts| {
                    default_artifacts | top_target.required_artifacts()
                });
            }

            top_deps.push(dep_targets);
        }

        // Merge the topological sorts.
        top_deps.sort_by_key(|deps| -(deps.len() as isize));
        for dep_targets in top_deps.into_iter() {
            for dep_target in dep_targets.into_iter() {
                if !build_plan.contains(&dep_target) {
                    build_plan.push(dep_target);
                }
            }
        }

        // Add the top targets to the build plan.
        for top_target in top_targets {
            if !build_plan.contains(top_target) {
                build_plan.push(top_target);
            }
        }

        Ok(build_plan)
    }

    /// Returns the reverse topologically sorted dependencies of this `Target`.
    pub fn dependencies_of<'a>(&'a self, target: &'a Target) -> Result<Vec<&'a Target>> {
        let mut sorted_deps = Vec::new();
        let mut unresolved_deps: Vec<(&Target, usize)> = Vec::new();
        unresolved_deps.push((target, 0));
        while let Some((dep, next_dep_idx)) = unresolved_deps.pop() {
            if let Some((transitive_dep_name, import_loc)) =
                dep.dependencies.iter().nth(next_dep_idx)
            {
                let lookup_base = dep.manifest_dir();
                let dep_name = &dep.name;
                let transitive_dep_target =
                    self.lookup_target(&transitive_dep_name, &import_loc, lookup_base)?;
                if unresolved_deps
                    .iter()
                    .any(|(ud, _)| *ud == transitive_dep_target)
                {
                    return Err(WorkspaceError::CircularDependency(
                        dep_name.to_string(),
                        transitive_dep_name.to_string(),
                    )
                    .into());
                } else {
                    unresolved_deps.push((dep, next_dep_idx + 1));
                    unresolved_deps.push((transitive_dep_target, 0));
                }
            } else if !sorted_deps.iter().any(|sd| *sd == dep) {
                sorted_deps.push(dep);
            }
        }

        sorted_deps.pop().unwrap(); // remove the target itself
        Ok(sorted_deps)
    }

    pub fn projects_of(&self, targets: &[&Target]) -> Vec<&Project> {
        let mut projects: Vec<&Project> = targets.iter().map(|t| t.project).collect();
        projects.sort_unstable_by_key(|p| *p as *const Project);
        projects.dedup_by_key(|p| *p as *const Project);
        projects
    }

    fn lookup_target(
        &self,
        name: &str,
        import_loc: &ImportLocation,
        import_base_path: &Path,
    ) -> Result<&Target> {
        let path = match import_loc {
            ImportLocation::Path(path) => canonicalize_path(import_base_path, path),
            _ => bail!("unsupported import location: {:?}", import_loc),
        };
        for proj in self.projects().iter() {
            if !path.starts_with(proj.manifest_path.parent().unwrap())
                && !path.starts_with(&proj.target_dir)
            {
                continue;
            }
            for target in proj.targets.iter() {
                if target.name == name {
                    return Ok(target);
                }
            }
        }
        Err(WorkspaceError::MissingDependency(format!("{} ({})", name, path.display())).into())
    }

    fn projects(&self) -> &[Pin<Box<Project>>] {
        unsafe { (&*self.projects.get()).as_slice() } // @see `struct Workspace`
    }

    fn load_projects_from_manifest(manifest_path: &Path) -> Result<Vec<Pin<Box<Project>>>> {
        debug!(
            "loading projects from manifest: {}",
            manifest_path.display()
        );
        let manifest_type = manifest_path
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or_else(|| {
                panic!(
                    "expected path to a Cargo.toml or package.json, but got {}",
                    manifest_path.display()
                )
            });
        match manifest_type {
            "Cargo.toml" => Self::load_cargo_projects(manifest_path),
            "package.json" => Self::load_javascript_projects(manifest_path),
            _ => Ok(Vec::new()),
        }
    }

    fn load_cargo_projects(manifest_path: &Path) -> Result<Vec<Pin<Box<Project>>>> {
        let metadata: CargoMetadata = serde_json::from_slice(
            &cmd!(
                "cargo",
                "metadata",
                "--manifest-path",
                manifest_path,
                "--no-deps",
                "--format-version=1"
            )?
            .stdout,
        )
        .map_err(|err| {
            anyhow::anyhow!(
                "unable to parse `{}`: {}. Are your Oasis dependencies properly specified?",
                manifest_path.display(),
                err
            )
        })?;

        let mut projects = Vec::new();
        for pkg in metadata.packages {
            let mut proj = Box::pin(Project {
                target_dir: metadata.target_directory.to_path_buf(),
                manifest_path: PathBuf::from(pkg.manifest_path),
                kind: ProjectKind::Rust,
                targets: Vec::new(),
            });
            let proj_ref = unsafe { &*(&*proj as *const Project) }; // @see `struct Workspace`
            for target in pkg.targets {
                let is_buildable = target.kind[0] == "bin"; // may include unit tests
                let is_testable = target.kind[0] == "test"; // integration tests

                let mut phases = Phases::CLEAN; // Cargo projects are always cleanable
                if is_buildable {
                    phases |= Phases::BUILD;
                }
                if is_buildable || is_testable {
                    phases |= Phases::TEST;
                }

                let deps = match &pkg.metadata {
                    Some(metadata) => {
                        let unpack_dep = |(name, loc): (&String, &ImportLocation)| {
                            (name.to_string(), loc.clone())
                        };
                        let oasis_meta = &metadata.oasis;
                        let mut deps: BTreeMap<_, _> = oasis_meta
                            .service_dependencies
                            .get(&target.name)
                            .map(|target_meta| {
                                target_meta.dependencies.iter().map(unpack_dep).collect()
                            })
                            .unwrap_or_default();
                        if is_testable {
                            deps.extend(oasis_meta.dev_dependencies.iter().map(unpack_dep));
                        }
                        deps
                    }
                    None => BTreeMap::default(),
                };
                proj.targets.push(Target {
                    project: proj_ref,
                    name: target.name.to_string(),
                    path: target.src_path,
                    phases,
                    dependencies: deps,
                    artifacts: Cell::new(Artifacts::SERVICE),
                    //^ TODO: move rust codegen and service detection to cli
                });
            }
            projects.push(proj);
        }
        Ok(projects)
    }

    fn load_javascript_projects(manifest_path: &Path) -> Result<Vec<Pin<Box<Project>>>> {
        let manifest: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&manifest_path)?)?;

        if manifest
            .get("devDependencies")
            .and_then(|deps| deps.get("lerna"))
            .map(|lerna| !lerna.is_null())
            .unwrap_or_default()
        {
            return Ok(Vec::new()); // there are subpackages to be found
        }

        let oasis_config = manifest.get("oasis").and_then(|oasis| oasis.as_object());

        let service_deps = oasis_config
            .and_then(|oasis| oasis.get("serviceDependencies"))
            .cloned()
            .and_then(|d| serde_json::from_value::<BTreeMap<String, String>>(d).ok())
            .unwrap_or_default();

        let mut phases = Phases::empty();
        if !service_deps.is_empty() {
            phases |= Phases::BUILD;
        }
        if let Some(scripts) = manifest.get("scripts").and_then(|s| s.as_object()) {
            if scripts.contains_key("build") {
                phases |= Phases::BUILD;
            }
            if scripts.contains_key("test") {
                phases |= Phases::TEST;
            }
            if scripts.contains_key("deploy") {
                phases |= Phases::DEPLOY;
            }
            if scripts.contains_key("clean") {
                phases |= Phases::CLEAN;
            }
        }
        if phases.is_empty() {
            return Ok(Vec::new()); // Nothing to be done. Ignore the package.
        }

        let manifest_dir = manifest_path.parent().unwrap();

        let clients_dir = oasis_config
            .and_then(|oasis| oasis.get("clientsDir"))
            .and_then(|clients_dir| clients_dir.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| manifest_dir.to_path_buf());

        let tsconfig: Option<serde_json::Map<String, serde_json::Value>> =
            fs::read(manifest_dir.join("tsconfig.json"))
                .ok()
                .and_then(|tsconfig| serde_json::from_slice(&tsconfig).ok());

        let mut proj = Box::pin(Project {
            kind: if tsconfig.is_some() {
                ProjectKind::TypeScript { clients_dir }
            } else {
                ProjectKind::JavaScript { clients_dir }
            },
            manifest_path: manifest_path.to_path_buf(),
            target_dir: tsconfig
                .and_then(|tsconfig| {
                    tsconfig
                        .get("compilerOptions")
                        .and_then(|opts| opts.as_object())
                        .and_then(|opts| opts.get("outDir")) // TODO: support `extends`
                        .map(|out_dir| {
                            canonicalize_path(manifest_dir, Path::new(out_dir.as_str().unwrap()))
                                .to_path_buf()
                        })
                })
                .unwrap_or_else(|| manifest_dir.to_path_buf()),
            targets: Vec::new(),
        });

        let proj_ref = unsafe { &*(&*proj as *const Project) }; // @see `struct Workspace`
        proj.targets.push(Target {
            name: manifest
                .get("name")
                .and_then(|name| name.as_str())
                .map(|name| name.to_string())
                .unwrap_or_default(),
            phases,
            project: proj_ref,
            dependencies: service_deps
                .into_iter()
                .map(|(name, loc)| {
                    let loc = if loc.starts_with("file:") {
                        ImportLocation::Path(
                            canonicalize_path(&manifest_dir, Path::new(&loc["file:".len()..]))
                                .to_path_buf(),
                        )
                    } else {
                        ImportLocation::Url(
                            url::Url::parse(&loc)
                                .map_err(|e| format_err!("invalid import url `{}`: {}", loc, e))?,
                        )
                    };
                    Ok((name, loc))
                })
                .collect::<Result<BTreeMap<_, _>>>()?,
            path: manifest_dir.to_path_buf(),
            artifacts: Cell::new(Artifacts::APP),
        });

        Ok(vec![proj])
    }
}

struct TopTargets<'a, 't> {
    workspace: &'a Workspace,

    /// Names of targets provided by the user.
    target_names: BTreeSet<&'t str>,

    /// Search paths for targets, as specified by the user.
    search_paths: BTreeMap<Cow<'t, Path>, &'t str>, // abs path -> user path

    /// Paths to raw Wasm targets
    wasm_paths: BTreeSet<&'t Path>,
}

impl<'a, 't> TopTargets<'a, 't> {
    fn new(workspace: &'a Workspace, target_strs: &'t [&'t str]) -> Self {
        let cwd = std::env::current_dir().unwrap(); // Checked during initialization.

        let mut target_names = BTreeSet::new();
        let mut search_paths = BTreeMap::new();
        let mut wasm_paths = BTreeSet::new();

        for target_str in target_strs {
            let target_path = Path::new(target_str);
            if target_str.ends_with(".wasm") || *target_str == "a.out" {
                wasm_paths.insert(target_path);
                continue;
            }
            if target_str.starts_with(":/") {
                search_paths.insert(
                    Cow::Owned(workspace.root.join(&target_str[2..])),
                    *target_str,
                );
            } else if target_str.starts_with('@') /* node module */ || target_str
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
            {
                target_names.insert(*target_str);
            } else if (target_str.contains('/') && target_path.exists()) || target_path.exists() {
                search_paths.insert(canonicalize_path(&cwd, target_path), *target_str);
            } else {
                warn!(
                    "`{}` does not refer to a target nor a directory containing targets",
                    target_str
                );
            }
        }

        Self {
            workspace,
            target_names,
            search_paths,
            wasm_paths,
        }
    }

    fn collect_targets(self) -> Result<Vec<&'a Target>> {
        let mut targets = Vec::new();
        self.collect_wasm_targets(&mut targets);
        self.collect_path_targets(&mut targets);
        self.collect_named_targets(&mut targets)?;
        Ok(targets)
    }

    fn collect_wasm_targets(&self, targets: &mut Vec<&'a Target>) {
        for path in self.wasm_paths.iter() {
            if !path.is_file() {
                warn!("`{}` does not exist", path.display());
                continue;
            }
            let mut proj = Box::pin(Project {
                target_dir: path.parent().unwrap().to_path_buf(),
                manifest_path: path.to_path_buf(),
                kind: ProjectKind::Wasm,
                targets: Vec::with_capacity(1),
            });
            let proj_ref = unsafe { &*(&*proj as *const Project) }; // @see `struct Workspace`
            proj.targets.push(Target {
                name: path.to_str().unwrap().to_string(),
                path: path.to_path_buf(),
                phases: Phases::BUILD,
                dependencies: BTreeMap::new(),
                project: proj_ref,
                artifacts: Cell::new(Artifacts::SERVICE),
            });
            unsafe { &mut *self.workspace.projects.get() }.push(proj); // @see `struct Workspace`
            targets.push(
                self.workspace
                    .projects()
                    .last()
                    .unwrap()
                    .targets
                    .first()
                    .unwrap(),
            );
        }
    }

    fn collect_path_targets(&self, targets: &mut Vec<&'a Target>) {
        for (path, target_str) in self.search_paths.iter() {
            if !path.exists() {
                warn!("the path `{}` does not exist", target_str);
                continue;
            }
            if !path.starts_with(&self.workspace.root) {
                warn!("the path `{}` exists outside of this workspace", target_str);
                continue;
            }
            let mut found_proj = false;
            for proj in self.workspace.projects().iter() {
                if proj.manifest_path.starts_with(path) {
                    found_proj = true;
                    targets.extend(proj.targets.iter());
                } else if path.starts_with(proj.manifest_path.parent().unwrap()) {
                    for target in proj.targets.iter() {
                        if target.path.starts_with(path) {
                            found_proj = true;
                            targets.push(target);
                        }
                    }
                }
            }
            if !found_proj {
                warn!("no services found in `{}`", target_str);
            }
        }
    }

    fn collect_named_targets(&self, targets: &mut Vec<&'a Target>) -> Result<()> {
        for target_name in self.target_names.iter() {
            let mut found_service = false;
            for p in self.workspace.projects().iter() {
                for target in p.targets.iter() {
                    if target.name == *target_name {
                        found_service = true;
                        targets.push(target);
                    }
                }
            }
            if !found_service {
                warn!("no target named `{}` found in the workspace", target_name);
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Project {
    pub target_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub kind: ProjectKind,
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug)]
pub enum ProjectKind {
    Rust,
    JavaScript { clients_dir: PathBuf },
    TypeScript { clients_dir: PathBuf },
    Wasm,
}

impl ProjectKind {
    pub fn name(&self) -> &str {
        match self {
            ProjectKind::Rust => "rust",
            ProjectKind::JavaScript { .. } => "javascript",
            ProjectKind::TypeScript { .. } => "typescript",
            ProjectKind::Wasm => "wasm",
        }
    }
}

pub struct Target {
    pub name: String,
    pub project: &'static Project,
    pub path: PathBuf,
    /// The development phases for which this target is relevant (e.g., build, deploy).
    phases: Phases,
    dependencies: BTreeMap<String, ImportLocation>,
    artifacts: Cell<Artifacts>,
}

impl Target {
    pub fn is_buildable(&self) -> bool {
        self.phases.contains(Phases::BUILD)
    }

    pub fn is_testable(&self) -> bool {
        self.phases.contains(Phases::TEST)
    }

    pub fn is_deployable(&self) -> bool {
        self.phases.contains(Phases::DEPLOY)
    }

    pub fn is_cleanable(&self) -> bool {
        self.phases.contains(Phases::CLEAN)
    }

    pub fn yields_artifact(&self, artifact: Artifacts) -> bool {
        self.artifacts.get().intersects(artifact)
    }

    pub fn manifest_dir(&self) -> &Path {
        self.project.manifest_path.parent().unwrap()
    }

    /// Retuns the path to where Oasis-generated dependencies should be placed.
    pub fn clients_dir(&self) -> PathBuf {
        let target_dir = &self.project.target_dir;
        match &self.project.kind {
            ProjectKind::Rust => target_dir.join("service"),
            ProjectKind::JavaScript { clients_dir } | ProjectKind::TypeScript { clients_dir } => {
                self.manifest_dir().join(clients_dir)
            }
            ProjectKind::Wasm => target_dir.to_path_buf(),
        }
    }

    /// Retuns the path to where Oasis build artifacts should be placed.
    pub fn artifacts_dir(&self) -> PathBuf {
        let target_dir = &self.project.target_dir;
        match self.project.kind {
            ProjectKind::Rust => target_dir.join("service"),
            _ => target_dir.to_path_buf(),
        }
    }

    pub fn wasm_path(&self) -> Option<PathBuf> {
        use heck::SnakeCase as _;
        if self.yields_artifact(Artifacts::SERVICE) {
            Some(match self.project.kind {
                ProjectKind::Rust => self
                    .artifacts_dir()
                    .join(format!("{}.wasm", self.name.to_snake_case())),
                //  ^ TODO: replace with kebab case once Rust codegen moves to cli
                _ => unreachable!(),
            })
        } else {
            None
        }
    }

    pub fn required_artifacts(&self) -> Artifacts {
        match self.project.kind {
            ProjectKind::Rust => Artifacts::RUST_CLIENT,
            ProjectKind::JavaScript { .. } | ProjectKind::TypeScript { .. } => {
                Artifacts::TYPESCRIPT_CLIENT
            }
            ProjectKind::Wasm => unimplemented!("cannot yet link wasm modules"),
        }
    }
}

impl PartialEq for Target {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && std::ptr::eq(self.project, other.project)
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Target")
            .field("name", &self.name)
            .field("project", &self.project.manifest_path)
            .field("dependencies", &self.dependencies)
            .field("artifacts", &self.artifacts)
            .finish()
    }
}

bitflags! {
    struct Phases: u8 {
        const BUILD  = 0b0000_0001;
        const TEST   = 0b0000_0010;
        const DEPLOY = 0b0000_0100;
        const CLEAN  = 0b0000_1000;
    }
}

bitflags! {
    pub struct Artifacts: u8 {
        const SERVICE           = 0b0000_0001;
        const APP               = 0b0000_0010;
        const RUST_CLIENT       = 0b0000_0100;
        const TYPESCRIPT_CLIENT = 0b0000_1000;
    }
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    #[serde(default)]
    packages: Vec<CargoPackage>,
    target_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    #[serde(default)]
    targets: Vec<CargoTarget>,
    manifest_path: String,
    #[serde(default)]
    metadata: Option<PackageMetadata>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

#[derive(Default, Debug, Deserialize)]
struct PackageMetadata {
    #[serde(default)]
    oasis: OasisMetadata,
}

type ServiceDependencies = BTreeMap<String, ImportLocation>;

#[derive(Default, Debug, Deserialize)]
struct OasisMetadata {
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: ServiceDependencies,
    #[serde(default, flatten)]
    service_dependencies: BTreeMap<String, OasisDeps>,
}

#[derive(Debug, Deserialize)]
struct OasisDeps {
    #[serde(default)]
    dependencies: ServiceDependencies,
}

/// Removes `.` and `..` from `path` given an already-dedotted `base` path.
fn canonicalize_path<'a>(base: &Path, path: &'a Path) -> Cow<'a, Path> {
    if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        let mut canon_path = base.to_path_buf();
        for comp in path.components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    canon_path.pop();
                }
                Component::Normal(c) => {
                    canon_path.push(c);
                }
                Component::RootDir => unreachable!("path is not absolute"),
                Component::Prefix(_) => unreachable!("Windows is not a supported OS"),
            }
        }
        Cow::Owned(canon_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonlicalize_abspath() {
        let base = Path::new("/");
        let abspath = Path::new("/lol/wtf/bbq");
        assert_eq!(canonicalize_path(&base, &abspath), abspath);
    }

    #[test]
    fn test_canonlicalize_relpath_below_base() {
        let base = Path::new("/a/path/somewhere");
        let abspath = Path::new(".././../test/.");
        assert_eq!(canonicalize_path(&base, &abspath), Path::new("/a/test"));
    }

    #[test]
    fn test_canonlicalize_relpath_above_base() {
        let base = Path::new("/a/path/");
        let abspath = Path::new("../../../../test");
        assert_eq!(canonicalize_path(&base, &abspath), Path::new("/test"));
    }
}
