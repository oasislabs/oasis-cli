use std::{collections::BTreeMap, ffi::OsString};

use colored::*;

use crate::{
    command::{BuildTool, Verbosity},
    config::{Config, DEFAULT_GATEWAY_URL},
    emit,
    errors::{ProfileError, ProfileErrorKind, Result},
    utils::{print_status_in, Status},
    workspace::{ProjectKind, Target, Workspace},
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
    pub fn new(m: &'a clap::ArgMatches, config: &Config) -> Result<Self> {
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
                return Err(anyhow::anyhow!(
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
    fn exec(self) -> Result<()> {
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
        deploy(&targets, self)
    }
}

pub fn deploy(targets: &[&Target], opts: DeployOptions) -> Result<()> {
    let mut found_deployable = false;
    for target in targets.iter().filter(|t| t.is_deployable()) {
        let proj = &target.project;
        match &proj.kind {
            ProjectKind::JavaScript { .. } | ProjectKind::TypeScript { .. } => {
                if opts.verbosity > Verbosity::Quiet {
                    print_status_in(
                        Status::Deploying,
                        &target.name,
                        proj.manifest_path.parent().unwrap(),
                    );
                }
                found_deployable = true;
                deploy_javascript(target, &opts)?
            }
            ProjectKind::Rust => {}
            _ => {}
        }
    }
    if !found_deployable {
        warn!("no deployable services found. Does your `package.json` contain a `deploy` script?");
    }
    Ok(())
}

fn deploy_javascript(target: &Target, opts: &DeployOptions) -> Result<()> {
    emit!(cmd.deploy.start, {
        "project_type": "js",
        "deployer_args": opts.deployer_args,
    });

    let mut args = Vec::new();
    if !opts.deployer_args.is_empty() {
        args.push("--");
        args.extend(opts.deployer_args.iter());
    }

    let mut envs = BTreeMap::new();
    envs.insert(
        OsString::from("OASIS_PROFILE"),
        OsString::from(&opts.profile),
    );
    if let Err(e) = BuildTool::for_target(target).deploy(args, envs, opts.verbosity) {
        emit!(cmd.deploy.error);
        return Err(e);
    }

    emit!(cmd.deploy.done);
    Ok(())
}
