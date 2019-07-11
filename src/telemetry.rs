use flate2::{write::GzEncoder, Compression};
use reqwest::header::{HeaderMap, HeaderValue};
use std::{fs, io::Write as _, path::Path};

use crate::{config::Telemetry, error::Error};

pub fn push(config: &Telemetry, dir: &Path) -> Result<(), failure::Error> {
    if !config.enabled || config.endpoint.is_empty() {
        return Ok(());
    }

    let entries = fs::read_dir(dir)?;
    let entry_count = entries.count();
    if entry_count < config.min_files {
        return Ok(());
    }

    debug!("collecting data from {} log files", entry_count);
    let mut count = 0;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Encoding", HeaderValue::from_static("gzip"));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .default_headers(headers)
        .build()?;

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

        let content = fs::read(file.path()).map_err(|err| {
            Error::ReadFile(file.path().to_str().unwrap().to_string(), err.to_string())
        })?;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&content[..])?;
        let res = client
            .post(&config.endpoint)
            .body(encoder.finish()?)
            .send()?;

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
