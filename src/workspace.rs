use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use oasis_rpc::import::ImportLocation;

use crate::{
    cmd,
    errors::{Error, WorkspaceError},
};

pub struct Workspace {
    root: PathBuf,
    projects: RefCell<Vec<Project>>,
}

impl Workspace {
    pub fn populate() -> Result<Self, Error> {
        let cwd = std::env::current_dir()?;
        let repo_root = cwd
            .ancestors()
            .find(|a| a.join(".git").is_dir())
            .ok_or_else(|| WorkspaceError::NotFound(cwd.display().to_string()))?;

        let walk_builder = ignore::WalkBuilder::new(repo_root);
        let walker = walk_builder.build().filter_map(|de| match de {
            Ok(de)
                if de.file_type().map(|ft| ft.is_file()).unwrap_or_default()
                    && (de.file_name() == "Cargo.toml" || de.file_name() == "package.json") =>
            {
                Some(de)
            }
            _ => None,
        });

        let mut projects = Vec::new();
        let mut project_dirs = Vec::new();
        for manifest_de in walker {
            let manifest_path = manifest_de.path();
            let mf_ty = manifest_de.file_name().to_owned();
            if project_dirs
                .iter()
                .any(|(m, p)| *m == mf_ty && manifest_path.starts_with(p))
            {
                continue;
            }
            let proj = Project::from_manifest(
                manifest_path,
                ProjectRef {
                    index: projects.len(),
                },
            )?;
            project_dirs.push((mf_ty, proj.manifest_path.parent().unwrap().to_path_buf()));
            projects.push(proj);
        }

        Ok(Self {
            root: repo_root.to_path_buf(),
            projects: RefCell::new(projects),
        })
    }

    pub fn collect_targets(&self, target_strs: &[&str]) -> Result<Vec<TargetRef>, Error> {
        let cwd = std::env::current_dir()?;
        let cwd_str = cwd.to_str().unwrap();
        let target_strs = if target_strs.is_empty() {
            std::borrow::Cow::Owned(vec![cwd_str])
        } else {
            std::borrow::Cow::Borrowed(target_strs)
        };

        let mut service_names = BTreeSet::new();
        let mut search_paths = BTreeMap::new();
        let mut wasm_paths = BTreeSet::new();

        for target_str in target_strs.iter() {
            let target_path = Path::new(target_str);
            if target_str.ends_with(".wasm") || *target_str == "a.out" {
                wasm_paths.insert(target_path);
                continue;
            }
            if target_str.starts_with(":/") {
                search_paths.insert(self.root.join(&target_str[2..]), target_str);
            } else if target_str.contains('/') || target_path.exists() {
                search_paths.insert(
                    if target_path.is_absolute() {
                        target_path.to_path_buf()
                    } else {
                        cwd.join(target_str)
                    },
                    target_str,
                );
            } else if target_str
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
            {
                service_names.insert(*target_str);
            } else {
                warn!(
                    "`{}` does not refer to a service nor a directory containing services",
                    target_str
                );
            }
        }

        let mut targets = Vec::new();

        for path in wasm_paths.iter() {
            if path.is_file() {
                let p_idx = self.projects.borrow().len();
                let target = Target {
                    name: path.to_str().unwrap().to_string(),
                    dependencies: BTreeMap::new(),
                    status: Cell::new(DependencyStatus::Resolved),
                    project_ref: ProjectRef { index: p_idx },
                };
                self.projects.borrow_mut().push(Project {
                    manifest_path: path.to_path_buf(),
                    kind: ProjectKind::Wasm,
                    targets: vec![target],
                });
                targets.push(TargetRef::new(p_idx, 0));
            } else {
                warn!("`{}` does not exist", path.display());
            }
        }

        for (path, target_str) in search_paths.iter() {
            let mut found_proj = false;
            for (p_idx, proj) in self.projects.borrow().iter().enumerate() {
                if proj.manifest_path.starts_with(path) {
                    found_proj = true;
                    targets
                        .extend((0..proj.targets.len()).map(|t_idx| TargetRef::new(p_idx, t_idx)));
                }
            }
            if !found_proj {
                warn!("no services found in `{}`", target_str);
            }
        }

        for service_name in service_names {
            let mut found_services = Vec::new();
            for (p_idx, p) in self.projects.borrow().iter().enumerate() {
                for (t_idx, t) in p.targets.iter().enumerate() {
                    if t.name == service_name {
                        found_services.push(TargetRef::new(p_idx, t_idx))
                    }
                }
            }
            if found_services.is_empty() {
                warn!("no service named `{}` found in the workspace", service_name);
            } else if found_services.len() > 1 {
                return Err(WorkspaceError::DuplicateService(service_name.to_string()).into());
            }
            targets.append(&mut found_services);
        }

        Ok(targets)
    }

    /// Returns the input targets in topologically sorted order.
    /// Returns an error if a dependency is missing or cyclic.
    pub fn construct_build_plan(&self, target_refs: &[TargetRef]) -> Result<Vec<TargetRef>, Error> {
        let mut build_plan = Vec::new();
        for target_ref in target_refs {
            self.resolve_dependencies_of(*target_ref, &mut build_plan)?;
        }
        Ok(build_plan)
    }

    pub fn projects_of(&self, target_refs: &[TargetRef]) -> Vec<ProjectRef> {
        let mut project_sel = vec![false; self.projects.borrow().len()];
        for &TargetRef { project_ref, .. } in target_refs.iter() {
            project_sel[project_ref.index] = true;
        }
        project_sel
            .into_iter()
            .enumerate()
            .filter_map(|(i, selected)| {
                if selected {
                    Some(ProjectRef { index: i })
                } else {
                    None
                }
            })
            .collect()
    }

    fn resolve_dependencies_of(
        &self,
        target_ref: TargetRef,
        build_plan: &mut Vec<TargetRef>,
    ) -> Result<(), Error> {
        let target = &self[target_ref];
        if let DependencyStatus::Resolved = target.status.get() {
            return Ok(());
        }
        target.status.replace(DependencyStatus::Visited);
        for (dep_name, dep) in target.dependencies.iter() {
            let mut dep = dep.borrow_mut();
            let dep_path = match &*dep {
                Dependency::Unresolved(ImportLocation::Path(path)) => {
                    if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        self[target.project_ref]
                            .manifest_path
                            .parent()
                            .unwrap()
                            .join(path)
                    }
                }
                _ => continue,
            };
            let dep_target_ref = self.lookup_target(&dep_name, &dep_path)?;
            let dep_target = &self[dep_target_ref];
            if let DependencyStatus::Visited = dep_target.status.get() {
                return Err(WorkspaceError::CircularDependency(
                    target.name.to_string(),
                    dep_target.name.to_string(),
                )
                .into());
            }
            self.resolve_dependencies_of(dep_target_ref, build_plan)?;
            *dep = Dependency::Resolved(dep_target_ref);
        }
        target.status.replace(DependencyStatus::Resolved);
        build_plan.push(target_ref);
        Ok(())
    }

    fn lookup_target(&self, name: &str, path: &Path) -> Result<TargetRef, Error> {
        for (p_idx, proj) in self.projects.borrow().iter().enumerate() {
            if !path.starts_with(proj.manifest_path.parent().unwrap()) {
                continue;
            }
            for (t_idx, target) in proj.targets.iter().enumerate() {
                if target.name == name {
                    return Ok(TargetRef::new(p_idx, t_idx));
                }
            }
        }
        // TODO: support building across workspaces
        Err(WorkspaceError::NotFound(format!("{} ({})", name, path.display())).into())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectRef {
    index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetRef {
    project_ref: ProjectRef,
    index: usize,
}

impl TargetRef {
    fn new(p_idx: usize, t_idx: usize) -> Self {
        TargetRef {
            project_ref: ProjectRef { index: p_idx },
            index: t_idx,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DependencyStatus {
    Unresolved,
    Visited,
    Resolved,
}

impl std::ops::Index<ProjectRef> for Workspace {
    type Output = Project;

    fn index(&self, p_ref: ProjectRef) -> &Self::Output {
        unsafe { &self.projects.try_borrow_unguarded().unwrap()[p_ref.index] }
        // ^ Projects are only appended and never, themselves, modified.
        //   The immutable borrow is always valid.
    }
}

impl std::ops::Index<TargetRef> for Workspace {
    type Output = Target;

    fn index(&self, t_ref: TargetRef) -> &Self::Output {
        &self[t_ref.project_ref].targets[t_ref.index]
    }
}

#[derive(Debug)]
pub struct Project {
    pub manifest_path: PathBuf,
    pub kind: ProjectKind,
    pub targets: Vec<Target>,
}

#[derive(Debug)]
pub enum ProjectKind {
    Rust { target_dir: PathBuf },
    JavaScript { deployable: bool, testable: bool },
    Wasm,
}

impl Project {
    fn from_manifest(manifest_path: &Path, project_ref: ProjectRef) -> Result<Self, Error> {
        match manifest_path.file_name().and_then(|p| p.to_str()) {
            Some("Cargo.toml") => {
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
                )?;
                let mut targets = Vec::new();
                for pkg in metadata.packages {
                    for target in pkg.targets {
                        if !target.kind.iter().any(|tk| tk == "bin") {
                            continue;
                        }
                        let deps = match &pkg.metadata {
                            Some(metadata) => metadata
                                .oasis
                                .get(&target.name)
                                .map(|target_meta| {
                                    target_meta
                                        .dependencies
                                        .iter()
                                        .map(|(name, loc)| {
                                            (
                                                name.to_string(),
                                                RefCell::new(Dependency::Unresolved(loc.clone())),
                                            )
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                            None => BTreeMap::default(),
                        };
                        targets.push(Target {
                            project_ref,
                            name: target.name.to_string(),
                            dependencies: deps,
                            status: Cell::new(DependencyStatus::Unresolved),
                        });
                    }
                }
                Ok(Self {
                    manifest_path: manifest_path.to_path_buf(),
                    kind: ProjectKind::Rust {
                        target_dir: metadata.target_directory,
                    },
                    targets,
                })
            }
            Some("package.json") => {
                let manifest: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
                // TODO: JS project dependency resolution
                let targets = if manifest
                    .get("devDependencies")
                    .and_then(|deps| deps.get("lerna"))
                    .map(|lerna| !lerna.is_null())
                    .unwrap_or_default()
                {
                    manifest_path
                        .parent()
                        .unwrap()
                        .join("packages")
                        .read_dir()
                        .map(|dir_ents| {
                            dir_ents
                                .filter_map(|de| match de {
                                    Ok(de) if de.file_type().ok()?.is_dir() => Some(Target {
                                        name: de.file_name().to_str().unwrap().to_string(),
                                        project_ref,
                                        dependencies: Default::default(),
                                        status: Cell::new(DependencyStatus::Resolved),
                                    }),
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    vec![Target {
                        name: manifest
                            .get("name")
                            .and_then(|name| name.as_str())
                            .map(|name| name.to_string())
                            .unwrap_or_default(),
                        project_ref,
                        dependencies: Default::default(),
                        status: Cell::new(DependencyStatus::Resolved),
                    }]
                };
                let npm_scripts = manifest.get("scripts").and_then(|s| s.as_object());
                Ok(Self {
                    manifest_path: manifest_path.to_path_buf(),
                    kind: ProjectKind::JavaScript {
                        testable: npm_scripts
                            .map(|s| s.contains_key("test"))
                            .unwrap_or_default(),
                        deployable: npm_scripts
                            .map(|s| s.contains_key("deploy"))
                            .unwrap_or_default(),
                    },
                    targets,
                })
            }
            _ => unreachable!(
                "`Project::from_manifest` requires a Cargo.toml or package.json, but received {}",
                manifest_path.display()
            ),
        }
    }
}

#[derive(Debug)]
pub struct Target {
    pub name: String,
    pub project_ref: ProjectRef,
    dependencies: BTreeMap<String, RefCell<Dependency>>,
    status: Cell<DependencyStatus>,
}

#[derive(Debug)]
enum Dependency {
    Unresolved(ImportLocation),
    Resolved(TargetRef),
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
    metadata: Option<OasisMetadata>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Default, Debug, Deserialize)]
struct OasisMetadata {
    #[serde(default)]
    oasis: BTreeMap<String, OasisDeps>,
}

#[derive(Debug, Deserialize)]
struct OasisDeps {
    #[serde(default)]
    dependencies: BTreeMap<String, ImportLocation>,
}
