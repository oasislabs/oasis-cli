use std::{
    cell::RefCell,
    fs::{File, OpenOptions},
    io::{prelude::*, BufReader},
    sync::Mutex,
};

use flate2::{write::GzEncoder, Compression};
use fs2::FileExt;
use once_cell::sync::OnceCell;

const DESTINATION_URL: &str = "https://gollum.devnet2.oasiscloud.io";
const UPLOAD_THRESHOLD_FILESIZE: u64 = 50 * 1024; // 50 KiB

static TLM: OnceCell<Telemetry> = OnceCell::new();

struct Telemetry {
    user_id: String,
    log_file: Mutex<RefCell<File>>,
    session_id: u32,
}

#[derive(serde::Serialize)]
struct Event {
    event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    timestamp: u64,
    session_id: u32,
}

pub fn init(config: &crate::config::Config) -> Result<(), failure::Error> {
    let tcfg = &config.telemetry;
    if !tcfg.enabled {
        return Ok(());
    }

    let metrics_path = metrics_path()?;

    if std::fs::metadata(&metrics_path)?.len() >= UPLOAD_THRESHOLD_FILESIZE {
        std::process::Command::new(std::env::args_os().nth(0).unwrap())
            .args(&["telemetry", "upload"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }

    TLM.set(Telemetry {
        user_id: tcfg.user_id.clone(),
        session_id: std::process::id(),
        log_file: Mutex::new(RefCell::new(
            OpenOptions::new()
                .create(true)
                .read(true)
                .append(true)
                .open(&metrics_path)
                .map_err(|err| crate::error::Error::OpenLogFile(err.to_string()))?,
        )),
    })
    .ok();
    Ok(())
}

pub fn metrics_path() -> Result<std::path::PathBuf, failure::Error> {
    Ok(crate::oasis_dir!(data)?.join("metrics.jsonl"))
}

pub fn __emit(event: &'static str, data: serde_json::Value) -> Result<(), failure::Error> {
    let Telemetry {
        session_id,
        log_file,
        ..
    } = match TLM.get() {
        Some(tlm) => tlm,
        None => return Ok(()),
    };

    let log_file = log_file.lock().unwrap();
    let mut log_file = log_file.borrow_mut();
    log_file.lock_shared()?;
    log_file.write_all(
        &serde_json::to_string(&Event {
            event,
            data: if data.as_array().unwrap().is_empty() {
                None
            } else {
                Some(data)
            },
            session_id: *session_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })?
        .as_bytes(),
    )?;
    log_file.flush()?;
    log_file.unlock()?;
    Ok(())
}

pub fn upload() -> Result<(), failure::Error> {
    let Telemetry {
        user_id, log_file, ..
    } = match TLM.get() {
        Some(tlm) => tlm,
        None => return Ok(()),
    };

    let log_file = log_file.lock().unwrap();
    let mut log_file = log_file.borrow_mut();

    // BEGIN CRITICAL SECTION
    log_file.lock_exclusive()?;

    log_file.seek(std::io::SeekFrom::Start(0))?;
    let mut rd = BufReader::new(&*log_file);
    let mut log = Vec::new();
    rd.read_to_end(&mut log)?;

    let try_upload = || -> Result<(), failure::Error> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::best());
        gz.write_all(&log)?;
        let body = gz.finish()?;

        let client = reqwest::Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    "Content-Encoding",
                    reqwest::header::HeaderValue::from_static("gzip"),
                );
                headers
            })
            .build()?;
        client
            .post(
                reqwest::Url::parse(DESTINATION_URL)
                    .unwrap()
                    .join(user_id)?,
            )
            .body(body)
            .send()?;
        Ok(())
    };

    let result = try_upload();
    if let Ok(()) = result {
        log_file.set_len(0)?;
    }
    log_file.unlock()?;
    // END CRITICAL SECTION

    result
}

#[macro_export]
macro_rules! emit {
    ( $event:expr$(, $( $data:tt ),+ )? ) => {
        let data = serde_json::json!([$($($data),+)?]);
        if let Err(err) = $crate::telemetry::__emit(stringify!($event), data) {
            info!("could not append to log: {}", err);
        }
    };
}
