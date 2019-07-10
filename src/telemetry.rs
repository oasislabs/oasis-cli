use std::{fs, path::PathBuf};

use crate::{config::Telemetry, error::Error};

pub fn collect(config: &Telemetry, dir: &PathBuf) -> Result<(), failure::Error> {
    if !config.enabled {
        return Ok(());
    }

    let entries = fs::read_dir(dir)?;
    let entry_count = entries.count();
    if entry_count < config.min_files {
        return Ok(());
    }

    debug!("collecting data from {} log files", entry_count);
    let mut count = 0;
    let client = reqwest::Client::new();
    for entry in fs::read_dir(dir)? {
        let file = entry?;
        if !file.metadata()?.is_file() {
            continue;
        }

        let ext = match file.path().extension() {
            Some(ext) => ext.to_os_string(),
            None => continue,
        };

        if ext != "stdout" && ext != "stderr" {
            continue;
        }

        let content = fs::read_to_string(file.path()).map_err(|err| {
            Error::ReadFile(file.path().to_str().unwrap().to_string(), err.to_string())
        })?;

        let res = client.post(&config.endpoint).body(content).send()?;

        trace!(
            "uploaded file `{}` with status `{}`",
            file.path().to_str().unwrap(),
            res.status()
        );
        count += 1;
        fs::remove_file(file.path())?;
    }

    debug!("uploaded `{}` logs", count);
    Ok(())
}
