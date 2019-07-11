use std::io::Write;

pub struct Logger {
    output_stream: Box<dyn Write>,
    log_file: Box<dyn Write>,
}

unsafe impl Send for Logger {}

impl Logger {
    pub fn new(
        name: String,
        id: String,
        output_stream: Box<dyn Write>,
        mut log_file: Box<dyn Write>,
    ) -> std::io::Result<Self> {
        write!(
            log_file,
            r#"{{"date": "{}", "name": "{}", "id": "{}", "data": ""#,
            chrono::Utc::now(),
            name,
            id
        )?;
        Ok(Self {
            output_stream,
            log_file,
        })
    }
}

impl Write for Logger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output_stream.write_all(buf)?;
        write!(
            self.log_file,
            "{}",
            String::from_utf8(buf.to_vec())
                .map_err(|_| std::io::ErrorKind::InvalidData)?
                .replace("\n", "\\n")
                .replace("\r", "\\r")
                .replace(r#"""#, r#"\""#)
        )?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.output_stream.flush()?;
        self.log_file.flush()?;
        Ok(())
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        writeln!(self.log_file, r#""}}"#).unwrap();
    }
}
