use std::io::{self, Cursor, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config::{FilenameConfig, Profile};
use crate::error::{Result, SpoutError};

pub const MAX_UPLOAD: u64 = 100 * 1024 * 1024;

pub struct CountingReader<R> {
    inner: R,
    tally: Arc<AtomicUsize>,
}

impl<R> CountingReader<R> {
    pub fn new(inner: R, tally: Arc<AtomicUsize>) -> Self {
        Self { inner, tally }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.tally.fetch_add(n, Ordering::Relaxed);
        Ok(n)
    }
}

pub fn generate_filename(
    cfg: Option<&FilenameConfig>,
    ext_override: Option<&str>,
    name_override: Option<&str>,
) -> Result<String> {
    if let Some(name) = name_override {
        let ext = ext_override.map(|ext| ext.trim_start_matches('.'));
        return Ok(match ext {
            Some(ext) if !ext.is_empty() => format!("{}.{}", strip_extension(name), ext),
            _ => name.to_string(),
        });
    }

    let mut stem = cfg.and_then(|c| c.prefix.clone()).unwrap_or_default();

    if let Some(n) = cfg.and_then(|c| c.random) {
        let byte_len = n.div_ceil(2);
        let mut buf = vec![0u8; byte_len];
        getrandom::fill(&mut buf).map_err(SpoutError::RngError)?;
        stem.push_str(&hex::encode(&buf)[..n]);
    }

    if stem.is_empty() {
        stem.push_str("ss");
    }

    let ext = ext_override
        .map(String::from)
        .or_else(|| cfg.and_then(|c| c.extension.clone()))
        .unwrap_or_else(|| "png".to_string());

    Ok(format!("{}.{}", stem, ext.trim_start_matches('.')))
}

fn strip_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

pub fn validate_filename(name: &str) -> Result<()> {
    if name.chars().any(|c| matches!(c, '/' | '\\' | '\r' | '\n' | '\0'))
        || name.contains("..")
        || name.to_ascii_lowercase().contains("%2f")
    {
        return Err(SpoutError::DangerousFilename(name.to_string()));
    }
    Ok(())
}

pub fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

pub fn read_stdin(yolo: bool) -> Result<Cursor<Vec<u8>>> {
    let mut buf = Vec::new();
    if yolo {
        io::stdin().lock().read_to_end(&mut buf)?;
    } else {
        let mut input = io::stdin().lock().take(MAX_UPLOAD + 1);
        input.read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_UPLOAD {
            return Err(SpoutError::InputTooLarge(MAX_UPLOAD / (1024 * 1024)));
        }
    }

    Ok(Cursor::new(buf))
}

pub fn send_request<R: Read + Send + 'static>(
    client: &reqwest::blocking::Client,
    profile: &Profile,
    url: reqwest::Url,
    body: R,
    filename: &str,
) -> Result<reqwest::blocking::Response> {
    let mut req = match profile.method.to_ascii_uppercase().as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        m => return Err(SpoutError::UnsupportedMethod(m.to_string())),
    };

    for h in &profile.headers {
        req = req.header(&h.key, &h.value);
    }

    let response = match profile.format.as_str() {
        "multipart" => {
            let mime = mime_guess::from_path(filename).first_or_octet_stream();
            let part = reqwest::blocking::multipart::Part::reader(body)
                .file_name(filename.to_string())
                .mime_str(mime.as_ref())?;

            let mut form = reqwest::blocking::multipart::Form::new();
            for f in &profile.fields {
                form = form.text(f.key.clone(), f.value.clone());
            }

            req.multipart(form.part(profile.file_field.clone(), part))
                .send()?
        }
        "binary" => {
            if !profile.fields.is_empty() {
                return Err(SpoutError::FieldsInBinaryFormat);
            }
            req.body(reqwest::blocking::Body::new(body)).send()?
        }
        fmt => return Err(SpoutError::UnsupportedFormat(fmt.to_string())),
    };

    Ok(response)
}
