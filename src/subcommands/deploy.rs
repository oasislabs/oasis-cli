use std::{collections::BTreeMap, ffi::OsString, path::Path};

use colored::*;

use crate::{
    command::{run_cmd_with_env, Verbosity},
    config::{Config, DEFAULT_GATEWAY_URL},
    emit,
    errors::{Error, ProfileError, ProfileErrorKind},
    utils::{print_status_in, Status},
    workspace::{ProjectKind, TargetRef, Workspace},
};

macro_rules! print_need_deploy_key_message {
    ($profile_name:expr) => {
        println!(
            r#"{preamble}
Getting one is easy: just head to

    {dashboard_url}

and locate the "API Token" field. It's right next to the "Click to reveal" button,
which you should indeed click. Copy the revealed text to your clipboard then run

    {config_cmd}

which will begin to read your credential from stdin. You should then paste your
API token in and hit enter. You're ready to try your deploy again!
"#,
            preamble = "You need an account to deploy on the Oasis Devnet.".yellow(),
            dashboard_url = "https://dashboard.oasiscloud.io/settings".cyan(),
            config_cmd = format!("oasis config profile.{}.credential -", $profile_name).cyan()
        )
    };
}

pub struct DeployOptions<'a> {
    pub targets: Vec<&'a str>,
    pub profile: &'a str,
    pub verbosity: Verbosity,
    pub deployer_args: Vec<&'a str>,
}

impl<'a> DeployOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches, config: &Config) -> Result<Self, Error> {
        let profile_name = m.value_of("profile").unwrap();
        match config.profile(profile_name) {
            Ok(_) => (),
            Err(ProfileError {
                kind: ProfileErrorKind::MissingKey("credential"),
                ..
            }) if config
                .profile_raw(profile_name)
                .and_then(|t| t.get("gateway"))
                .and_then(|gw| gw.as_str())
                .map(|gw| gw == DEFAULT_GATEWAY_URL)
                .unwrap_or_default() =>
            {
                print_need_deploy_key_message!(profile_name);
                return Err(failure::format_err!(
                    "`profile.{}.credential` must be set to deploy on the Oasis Devnet.",
                    profile_name
                ));
            }
            Err(e) => return Err(e.into()),
        }
        Ok(Self {
            profile: profile_name,
            targets: m.values_of("TARGETS").unwrap_or_default().collect(),
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
            deployer_args: m.values_of("deployer_args").unwrap_or_default().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for DeployOptions<'a> {
    fn exec(self) -> Result<(), Error> {
        let workspace = Workspace::populate()?;
        let targets = workspace.collect_targets(&self.targets)?;
        let build_opts = super::BuildOptions {
            targets: self.targets.clone(),
            debug: false,
            verbosity: self.verbosity,
            stack_size: None,
            wasi: false,
            builder_args: Vec::new(),
        };
        super::build(&workspace, &targets, build_opts)?;
        deploy(&workspace, &targets, self)
    }
}

pub fn deploy(
    workspace: &Workspace,
    targets: &[TargetRef],
    opts: DeployOptions,
) -> Result<(), failure::Error> {
    for target_ref in targets {
        let target = &workspace[*target_ref];
        let proj = &workspace[target.project_ref];
        match &proj.kind {
            ProjectKind::JavaScript { deployable, .. } if *deployable => {
                if opts.verbosity > Verbosity::Quiet {
                    print_status_in(
                        Status::Deploying,
                        &target.name,
                        proj.manifest_path.parent().unwrap(),
                    );
                }
                deploy_js(&proj.manifest_path, &opts)?
            }
            _ => (),
        }
    }
    Ok(())
}

fn deploy_js(manifest_path: &Path, opts: &DeployOptions) -> Result<(), failure::Error> {
    let package_dir = manifest_path.parent().unwrap();

    emit!(cmd.deploy.start, {
        "project_type": "js",
        "deployer_args": opts.deployer_args,
    });

    let mut npm_args = vec![
        "run",
        "deploy",
        "--if-present",
        "--prefix",
        package_dir.to_str().unwrap(),
        "--",
    ];
    if opts.verbosity < Verbosity::Normal {
        npm_args.push("--silent");
    } else if opts.verbosity >= Verbosity::Verbose {
        npm_args.push("--verbose");
    }
    npm_args.extend(opts.deployer_args.iter());

    let mut npm_envs: BTreeMap<_, _> = std::env::vars_os().collect();
    npm_envs.insert(
        OsString::from("OASIS_PROFILE"),
        OsString::from(&opts.profile),
    );
    if let Err(e) = run_cmd_with_env("npm", npm_args, npm_envs, opts.verbosity) {
        emit!(cmd.deploy.error);
        return Err(e);
    }

    emit!(cmd.deploy.done);
    Ok(())
}
