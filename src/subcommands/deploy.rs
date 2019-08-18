use std::{ffi::OsString, path::Path};

use colored::*;

use crate::{
    command::{run_cmd_with_env, Verbosity},
    config::{Config, DEFAULT_GATEWAY_URL},
    emit,
    error::{ProfileError, ProfileErrorKind},
    utils::{detect_projects, print_status_in, ProjectKind, Status},
};

macro_rules! print_need_deploy_key_message {
    ($profile_name:expr) => {
        println!(
            r#"{preamble}
Getting one is easy: just head to

    {dashboard_url}

and locate the "Private Key" field. It's right next to the "Click to reveal" button,
which you should indeed click. Copy the revealed text to your clipboard then run

    {config_cmd}

Once you clear your clipboard, you can try your deploy again!
"#,
            preamble = "You need an account to deploy on the Oasis Devnet.".yellow(),
            dashboard_url = "https://dashboard.oasiscloud.io/settings#payments".cyan(),
            config_cmd =
                format!("oasis config profile.{}.private_key <paste>", $profile_name).cyan()
        )
    };
}

pub struct DeployOptions<'a> {
    profile: &'a str,
    verbosity: Verbosity,
    deployer_args: Vec<&'a str>,
}

impl<'a> DeployOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches, config: &Config) -> Result<Self, failure::Error> {
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
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
            deployer_args: m.values_of("deployer_args").unwrap_or_default().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for DeployOptions<'a> {
    fn exec(self) -> Result<(), failure::Error> {
        deploy(self)
    }
}

pub fn deploy(opts: DeployOptions) -> Result<(), failure::Error> {
    let mut found_deployable = false;
    for proj in detect_projects()? {
        match proj.kind {
            ProjectKind::Rust(_) => (),
            ProjectKind::Javascript(manifest) => {
                found_deployable = true;
                deploy_js(&opts, &proj.manifest_path, manifest)?;
            }
        }
    }
    if !found_deployable {
        return Err(failure::format_err!(
            "could not find any deployment scripts in project"
        ));
    }
    Ok(())
}

fn deploy_js(
    opts: &DeployOptions,
    manifest_path: &Path,
    package_json: serde_json::Map<String, serde_json::Value>,
) -> Result<(), failure::Error> {
    let package_dir = manifest_path.parent().unwrap();

    if opts.verbosity > Verbosity::Quiet {
        print_status_in(
            Status::Deploying,
            package_json["name"].as_str().unwrap(),
            package_dir,
        );
    }

    emit!(cmd.deploy.start, {
        "project_type": "js",
        "deployer_args": opts.deployer_args,
    });

    let mut npm_args = vec![
        "run",
        "deploy",
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

    let mut npm_envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
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
