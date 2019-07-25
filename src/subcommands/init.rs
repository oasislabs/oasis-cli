use std::{io::BufRead as _, path::PathBuf};

use colored::*;

use crate::{command::Verbosity, emit, error::Error};

const QUICKSTART_URL: &str = "https://github.com/oasislabs/quickstart";
const QUICKSTART_TGZ_BYTES: &[u8] = include_bytes!(env!("QUICKSTART_INCLUDE_PATH"));

pub struct InitOptions<'a> {
    project_type: &'a str,
    dest: PathBuf,
    verbosity: Verbosity,
}

impl<'a> InitOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches) -> Result<Self, failure::Error> {
        let project_type = match m.value_of("type").map(|t| t.trim()) {
            Some(t) if t.is_empty() => t,
            _ => "rust",
        };

        Ok(Self {
            project_type,
            dest: PathBuf::from(m.value_of("NAME").unwrap_or(".")),
            verbosity: Verbosity::from(m.occurrences_of("verbose")),
        })
    }
}

impl<'a> super::ExecSubcommand for InitOptions<'a> {
    fn exec(self) -> Result<(), failure::Error> {
        init(self)
    }
}

/// Creates an Oasis project in a directory.
pub fn init(opts: InitOptions) -> Result<(), failure::Error> {
    let project_type_display = if opts.verbosity >= Verbosity::Normal {
        opts.project_type[0..1].to_uppercase() + &opts.project_type[1..]
    } else {
        String::new()
    };
    match opts.project_type {
        "rust" => init_rust(opts),
        project_type => Err(Error::UnknownProjectType(project_type.to_string()).into()),
    }?;
    eprintln!(
        "     {} {} service",
        "Created".green(),
        project_type_display
    );
    Ok(())
}

fn init_rust(opts: InitOptions) -> Result<(), failure::Error> {
    let dest = &opts.dest;
    if dest.exists() {
        return Err(Error::FileAlreadyExists(dest.display().to_string()).into());
    }

    match git2::Repository::clone(QUICKSTART_URL, dest) {
        Ok(_) => {
            emit!(cmd.init, { "project_type": "rust", "source": "repo" });
            std::fs::remove_dir_all(dest.join(".git"))?;
        }
        Err(err) => {
            emit!(cmd.init, {
                "project_type": "rust",
                "source": "archive",
                "repo_err": err.to_string()
            });
            debug!("Could not clone quickstart repo: {}", err);
            let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(QUICKSTART_TGZ_BYTES));
            for entry in ar.entries()? {
                let mut entry = entry?;
                let entry_path = entry.path()?;
                let file_path = match entry_path.strip_prefix("quickstart-master/") {
                    Ok(suffix) if !suffix.ends_with("/") => dest.join(suffix),
                    _ => continue,
                };
                entry.unpack(file_path)?;
            }
        }
    }
    git2::Repository::init(dest)?;

    let project_name = dest
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace("_", "-");

    std::fs::write(dest.join("README.md"), format!("# {}", project_name))?;

    let manifest_path = dest.join("service/Cargo.toml");
    let manifest_lines = std::io::BufReader::new(std::fs::File::open(&manifest_path)?)
        .lines()
        .map(|line| {
            let line = line?;
            Ok(if line.starts_with("authors = [") {
                "authors = []".to_string()
            } else if line.starts_with("name = ") {
                format!("name = \"{}\"", project_name)
            } else {
                line
            })
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;

    std::fs::write(&manifest_path, manifest_lines.join("\n"))?;

    let main_path = dest.join("service/src/main.rs");
    replace_quickstart(
        main_path,
        inflector::cases::classcase::to_class_case(&project_name),
    )?;

    let package_path = dest.join("app/package.json");
    replace_quickstart(
        package_path,
        project_name.clone()
    )?;

    let test_path = dest.join("app/test/service.spec.ts");
    replace_quickstart(
        test_path,
        inflector::cases::classcase::to_class_case(&project_name),
    )?;

    Ok(())
}

fn replace_quickstart(
    file_path: std::path::PathBuf,
    replacement: String,
) -> Result<(), failure::Error> {
    let mut buffer = std::fs::read_to_string(&file_path)?;
    buffer = buffer
        .replace("Quickstart", &replacement)
        .replace("quickstart", &replacement);

    std::fs::write(&file_path, buffer)?;

    Ok(())
}
