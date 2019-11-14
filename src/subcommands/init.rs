use std::{
    fs,
    path::{Path, PathBuf},
};

use heck::{CamelCase, SnakeCase};

use crate::{
    cmd,
    command::Verbosity,
    emit,
    errors::{CliError, Result},
    utils::{print_status_in, Status},
};

const TEMPLATE_REPO_URL: &str = "https://github.com/oasislabs/template";
const TEMPLATE_TGZ_BYTES: &[u8] = include_bytes!(env!("TEMPLATE_INCLUDE_PATH"));

pub struct InitOptions<'a> {
    project_type: &'a str,
    dest: PathBuf,
    verbosity: Verbosity,
}

impl<'a> InitOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches) -> Result<Self> {
        let project_type = match m.value_of("type").map(|t| t.trim()) {
            Some(t) if t.is_empty() => t,
            _ => "rust",
        };

        Ok(Self {
            project_type,
            dest: PathBuf::from(m.value_of("NAME").unwrap_or(".")),
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
        })
    }
}

impl<'a> super::ExecSubcommand for InitOptions<'a> {
    fn exec(self) -> Result<()> {
        init(self)
    }
}

/// Creates an Oasis project in a directory.
pub fn init(opts: InitOptions) -> Result<()> {
    let project_type_display =
        opts.project_type[0..1].to_uppercase() + &opts.project_type[1..] + " project";
    match opts.project_type {
        "rust" => init_rust(&opts),
        _ => unreachable!(),
    }?;
    if opts.verbosity > Verbosity::Quiet {
        print_status_in(Status::Created, project_type_display, &opts.dest);
    }
    Ok(())
}

fn init_rust(opts: &InitOptions) -> Result<()> {
    let dest = &opts.dest;
    if dest.exists() {
        return Err(CliError::FileAlreadyExists(dest.display().to_string()).into());
    }
    fs::create_dir_all(dest)?;

    match clone_template_repo(dest) {
        Ok(_) => {
            emit!(cmd.init, { "type": "rust", "source": "repo" });
        }
        Err(err) => {
            emit!(cmd.init, { "type": "rust", "source": "tgz", "repo_err": err.to_string() });
            debug!("Could not clone template repo: {}", err);
            unpack_template_tgz(dest)
                .map_err(|err| anyhow!("Could not unpack template archive: {}", err))?;
        }
    }
    match cmd!("git", "rev-parse", "--git-dir") {
        Ok(_) => {
            fs::remove_dir_all(dest.join(".github")).ok();
        }
        Err(_) => {
            cmd!("git", "init", dest)?;
        }
    }

    let project_name = dest
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace("_", "-");

    std::fs::write(dest.join("README.md"), format!("# {}", project_name))?;

    rename_project(dest, &project_name)?;

    Ok(())
}

fn clone_template_repo(dest: &Path) -> Result<()> {
    let dest = dest.canonicalize()?;
    cmd!("git", "clone", TEMPLATE_REPO_URL, &dest)?;
    let orig_dir = std::env::current_dir()?;
    std::env::set_current_dir(&dest)?;
    let do_clone = || {
        let version_req = semver::VersionReq::parse(env!("TEMPLATE_VER")).unwrap();
        let tags_str = String::from_utf8(cmd!("git", "tag", "-l", "v*.*.*")?.stdout).unwrap();
        let best_tag = tags_str
            .trim()
            .split('\n')
            .filter_map(|t| {
                let ver = semver::Version::parse(&t[1..]).expect(t);
                if version_req.matches(&ver) {
                    Some((ver, t))
                } else {
                    None
                }
            })
            .max()
            .unwrap()
            .1;
        cmd!("git", "reset", "--hard", best_tag)?;
        std::fs::remove_dir_all(dest.join(".git"))?;
        Ok(())
    };
    let result = do_clone();
    std::env::set_current_dir(orig_dir)?;
    result
}

fn unpack_template_tgz(dest: &Path) -> Result<()> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(TEMPLATE_TGZ_BYTES));
    for entry in ar.entries()? {
        let mut entry = entry?;
        entry.unpack(dest.join(entry.path()?)).unwrap();
    }
    Ok(())
}

fn rename_project(dir: &Path, project_name: &str) -> Result<()> {
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
