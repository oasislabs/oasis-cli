use std::{
    cell::{Cell, RefCell, UnsafeCell},
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    pin::Pin,
};

use oasis_rpc::import::ImportLocation;

use crate::{
    cmd,
    errors::{Error, WorkspaceError},
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
    pub fn populate() -> Result<Self, Error> {
        let cwd = std::env::current_dir()?;
        let repo_root = cwd
            .ancestors()
            .find(|a| a.join(".git").is_dir())
            .ok_or_else(|| WorkspaceError::NoWorkspace(cwd.display().to_string()))?;

        let walk_builder = ignore::WalkBuilder::new(repo_root);
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
        let mut project_dirs = Vec::new();
        for manifest_de in manifest_walker {
            let manifest_path = manifest_de.path();
            let mf_ty = manifest_de.file_name().to_owned();
            if project_dirs
                .iter()
                .any(|(m, p)| *m == mf_ty && manifest_path.starts_with(p))
            {
                continue;
            }
            let proj = Project::from_manifest(manifest_path)?;
            project_dirs.push((mf_ty, proj.manifest_path.parent().unwrap().to_path_buf()));
            projects.push(proj);
        }

        Ok(Self {
            root: repo_root.to_path_buf(),
            projects: UnsafeCell::new(projects),
        })
    }

    /// Collects the set of top-level dependencies that are matched by the input `target_strs`.
    /// A valid target str is either the name of a service or a path in the workspace that
    /// points to a directory that contains services. Like git, `:/` refers to the workspace root.
    pub fn collect_targets(&self, target_strs: &[&str]) -> Result<Vec<&Target>, Error> {
        let cwd = std::env::current_dir()?;
        let target_strs = if target_strs.is_empty() {
            &["."]
        } else {
            target_strs
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
            if !path.is_file() {
                warn!("`{}` does not exist", path.display());
                continue;
            }
            let mut proj = Box::pin(Project {
                manifest_path: path.to_path_buf(),
                kind: ProjectKind::Wasm,
                targets: Vec::with_capacity(1),
            });
            let proj_ref = unsafe { &*(&*proj as *const Project) }; // @see `struct Workspace`
            proj.targets.push(Target {
                name: path.to_str().unwrap().to_string(),
                dependencies: BTreeMap::new(),
                status: Cell::new(DependencyStatus::Resolved),
                project: proj_ref,
            });
            unsafe { &mut *self.projects.get() }.push(proj); // @see `struct Workspace`
        }

        for (path, target_str) in search_paths.iter() {
            let mut found_proj = false;
            for proj in self.projects().iter() {
                if proj.manifest_path.starts_with(path) {
                    found_proj = true;
                    targets.extend(proj.targets.iter());
                }
            }
            if !found_proj {
                warn!("no services found in `{}`", target_str);
            }
        }

        for service_name in service_names {
            let mut found_services = Vec::new();
            for p in self.projects().iter() {
                for target in p.targets.iter() {
                    if target.name == service_name {
                        found_services.push(target);
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
    pub fn construct_build_plan<'a>(
        &'a self,
        targets: &[&'a Target],
    ) -> Result<Vec<&'a Target>, Error> {
        let mut build_plan = Vec::new();
        for target in targets {
            self.resolve_dependencies_of(target, &mut build_plan)?;
        }
        Ok(build_plan)
    }

    pub fn projects_of(&self, targets: &[&Target]) -> Vec<&Project> {
        let mut projects: Vec<&Project> = targets.iter().map(|t| t.project).collect();
        projects.sort_unstable_by_key(|p| *p as *const Project);
        projects.dedup_by_key(|p| *p as *const Project);
        projects
    }

    fn resolve_dependencies_of<'a>(
        &'a self,
        target: &'a Target,
        build_plan: &mut Vec<&'a Target>,
    ) -> Result<(), Error> {
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
                        target.project.manifest_path.parent().unwrap().join(path)
                    }
                }
                _ => continue,
            };
            let dep_target = self.lookup_target(&dep_name, &dep_path)?;
            if let DependencyStatus::Visited = dep_target.status.get() {
                return Err(WorkspaceError::CircularDependency(
                    target.name.to_string(),
                    dep_target.name.to_string(),
                )
                .into());
            }
            self.resolve_dependencies_of(dep_target, build_plan)?;
            *dep =
                Dependency::Resolved(unsafe { std::mem::transmute::<&_, &'static _>(dep_target) });
            // ^ @see `enum Dependency`
        }
        target.status.replace(DependencyStatus::Resolved);
        build_plan.push(target);
        Ok(())
    }

    fn lookup_target(&self, name: &str, path: &Path) -> Result<&Target, Error> {
        for proj in self.projects().iter() {
            if !path.starts_with(proj.manifest_path.parent().unwrap()) {
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
}

#[derive(Clone, Copy, Debug)]
enum DependencyStatus {
    Unresolved,
    Visited,
    Resolved,
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
    fn from_manifest(manifest_path: &Path) -> Result<Pin<Box<Self>>, Error> {
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

                let mut proj = Box::pin(Self {
                    manifest_path: manifest_path.to_path_buf(),
                    kind: ProjectKind::Rust {
                        target_dir: metadata.target_directory,
                    },
                    targets: Vec::new(),
                });
                let proj_ref = unsafe { &*(&*proj as *const Project) }; // @see `struct Workspace`

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
                        proj.targets.push(Target {
                            project: proj_ref,
                            name: target.name.to_string(),
                            dependencies: deps,
                            status: Cell::new(DependencyStatus::Unresolved),
                        });
                    }
                }
                Ok(proj)
            }
            Some("package.json") => {
                let manifest: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_slice(&std::fs::read(&manifest_path)?)?;

                let npm_scripts = manifest.get("scripts").and_then(|s| s.as_object());
                let mut proj = Box::pin(Self {
                    manifest_path: manifest_path.to_path_buf(),
                    kind: ProjectKind::JavaScript {
                        testable: npm_scripts
                            .map(|s| s.contains_key("test"))
                            .unwrap_or_default(),
                        deployable: npm_scripts
                            .map(|s| s.contains_key("deploy"))
                            .unwrap_or_default(),
                    },
                    targets: Vec::new(),
                });
                let proj_ref = unsafe { &*(&*proj as *const Project) }; // @see `struct Workspace`

                proj.targets = if manifest
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
                                        project: proj_ref,
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
                        project: proj_ref,
                        dependencies: Default::default(),
                        status: Cell::new(DependencyStatus::Resolved),
                    }]
                };

                Ok(proj)
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
    pub project: &'static Project,
    dependencies: BTreeMap<String, RefCell<Dependency>>,
    status: Cell<DependencyStatus>,
}

#[derive(Debug)]
enum Dependency {
    Unresolved(ImportLocation),

    // The `'static` is with respect to the `Target` that will forever own this `Dependency`
    Resolved(&'static Target),
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
