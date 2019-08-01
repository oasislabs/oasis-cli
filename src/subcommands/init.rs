use std::path::{Path, PathBuf};

use colored::*;
use heck::{CamelCase, SnakeCase};

use crate::{command::Verbosity, emit, error::Error};

const TEMPLATE_URL: &str = "https://github.com/oasislabs/template";
const TEMPLATE_TGZ_BYTES: &[u8] = include_bytes!(env!("TEMPLATE_INCLUDE_PATH"));

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

    match clone_template_repo(dest) {
        Ok(_) => {
            emit!(cmd.init, { "type": "rust", "source": "repo" });
        }
        Err(err) => {
            emit!(cmd.init, { "type": "rust", "source": "tgz", "repo_err": err.to_string() });
            debug!("Could not clone template repo: {}", err);
            unpack_template_tgz(dest).map_err(|err| {
                failure::format_err!("Could not unpack template archive: {}", err)
            })?;
        }
    }
    git2::Repository::init(dest)?;

    let project_name = dest
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace("_", "-");

    std::fs::write(dest.join("README.md"), format!("# {}", project_name))?;

    rename_project(dest, &project_name)?;

    Ok(())
}

fn clone_template_repo(dest: &Path) -> Result<(), failure::Error> {
    let repo = git2::Repository::clone(TEMPLATE_URL, dest)?;
    let version_req = semver::VersionReq::parse(env!("TEMPLATE_VER")).unwrap();
    let tag_names = repo.tag_names(Some("v*"))?;
    let tag = tag_names
        .iter()
        .find(|tag_name| {
            semver::Version::parse(&tag_name.unwrap()[1..] /* strip leading 'v' */)
                .map(|v| version_req.matches(&v))
                .unwrap_or_default()
        })
        .unwrap() // build would have errored if the tag didn't exist
        .unwrap(); // tag name is always utf8
    repo.reset(
        &repo
            .find_reference(&format!("refs/tags/{}", tag))?
            .peel_to_commit()?
            .as_object(),
        git2::ResetType::Hard,
        None, /* checkout builder */
    )?;
    std::fs::remove_dir_all(dest.join(".git"))?;
    Ok(())
}

fn unpack_template_tgz(dest: &Path) -> Result<(), failure::Error> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(TEMPLATE_TGZ_BYTES));
    for entry in ar.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?;
        let file_path = dest.join(entry_path.iter().skip(1).collect::<PathBuf>());
        entry.unpack(file_path)?;
    }
    Ok(())
}

fn rename_project(dir: &Path, project_name: &str) -> Result<(), failure::Error> {
    let project_name = project_name.to_snake_case();
    let service_name = project_name.to_camel_case();
    for f in walkdir::WalkDir::new(dir).into_iter() {
        let f = f?;
        if !f.file_type().is_file() {
            continue;
        }
        let p = f.path();
        std::fs::write(
            p,
            std::fs::read_to_string(p)?
                .replace("quickstart", &project_name)
                .replace("Quickstart", &service_name),
        )?;
    }
    Ok(())
}
