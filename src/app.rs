use std::io::{self, IsTerminal, Read};
use std::time::Duration;

use crate::cli::Cli;
use crate::clipboard::run_clipboard;
use crate::config::{load_config, write_config};
use crate::error::{Result, SpoutError};
use crate::net::{
    HTTP_TIMEOUT_SECS, MAX_RESPONSE, extract_response_value, parse_http_url, resolve_url,
    uri_encode, validate_response_url,
};
use crate::sanitize::{SanitizeStatus, sanitize_media};
use crate::upload::{generate_filename, read_stdin, send_request, validate_filename};

pub fn run() -> Result<()> {
    let cli = Cli::parse()?;

    if cli.gen_config_force {
        return write_config(true);
    }
    if cli.gen_config {
        return write_config(false);
    }
    if cli.check {
        load_config()?;
        eprintln!("spout: config ok");
        return Ok(());
    }

    if io::stdin().is_terminal() {
        return Err(SpoutError::NoInputData);
    }

    let config = load_config()?;
    let target = cli.profile.unwrap_or_else(|| config.default.clone());

    let profile = config
        .profiles
        .into_iter()
        .find(|p| p.name == target)
        .ok_or_else(|| SpoutError::ProfileNotFound(target.clone()))?;

    let filename = generate_filename(
        profile.filename.as_ref(),
        cli.ext.as_deref(),
        cli.name.as_deref(),
    )?;

    if !config.yolo {
        validate_filename(&filename)?;
    }

    let raw_url = profile.url.replace("{filename}", &uri_encode(&filename));

    let mut builder = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .http1_only()
        .user_agent(concat!("spout/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS));

    let url = if config.yolo {
        builder = builder
            .redirect(reqwest::redirect::Policy::limited(10))
            .danger_accept_invalid_certs(true);
        parse_http_url(&raw_url)?
    } else {
        let (parsed, addr) = resolve_url(&raw_url)?;
        builder = builder
            .resolve(parsed.host_str().unwrap_or(""), addr)
            .redirect(reqwest::redirect::Policy::none());
        parsed
    };

    let client = builder.build()?;
    let t0 = std::time::Instant::now();
    let sanitized = sanitize_media(
        read_stdin(config.yolo)?.into_inner(),
        profile.strip_meta.unwrap_or(true),
    )?;
    let clean_status = format_clean_status(sanitized.status);
    let uploaded = sanitized.bytes.len();

    let mut response = send_request(
        &client,
        &profile,
        url,
        std::io::Cursor::new(sanitized.bytes),
        &filename,
    )?;

    let elapsed = t0.elapsed().as_secs_f64();
    let status = response.status();

    let raw_body = if config.yolo {
        let mut raw_body = Vec::new();
        response
            .read_to_end(&mut raw_body)
            .map_err(SpoutError::ResponseReadError)?;
        raw_body
    } else {
        read_limited_body(response.take(MAX_RESPONSE + 1), MAX_RESPONSE)?
    };

    let body = String::from_utf8_lossy(&raw_body);

    if !status.is_success() {
        return Err(SpoutError::UploadFailed(
            status,
            limit_error_body(body.trim()),
        ));
    }

    let result = extract_response_value(&body, &profile.path)?;

    if !config.yolo {
        validate_response_url(&result)?;
    }

    eprintln!(
        "spout: ok [{}] {} {} B {:.1}s clean={}",
        target, filename, uploaded, elapsed, clean_status
    );
    println!("{}", result);

    if !cli.no_clipboard
        && let Some(cmd) = config.clipboard
        && !cmd.is_empty()
        && let Err(e) = run_clipboard(&cmd, &result, config.yolo)
    {
        eprintln!("spout: warn: clipboard error: {}", e);
    }

    Ok(())
}

fn format_clean_status(status: SanitizeStatus) -> &'static str {
    match status {
        SanitizeStatus::Cleaned => "cleaned",
        SanitizeStatus::Kept => "kept",
        SanitizeStatus::Unknown => "unsupported",
        SanitizeStatus::Disabled => "off",
    }
}

fn limit_error_body(body: &str) -> String {
    const MAX_ERROR_BODY_LEN: usize = 4096;

    if body.len() <= MAX_ERROR_BODY_LEN {
        return body.to_string();
    }

    let mut end = MAX_ERROR_BODY_LEN;
    while !body.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...", &body[..end])
}

fn read_limited_body<R: Read>(mut reader: R, limit: u64) -> Result<Vec<u8>> {
    let mut raw_body = Vec::new();
    reader
        .read_to_end(&mut raw_body)
        .map_err(SpoutError::ResponseReadError)?;

    if raw_body.len() as u64 > limit {
        return Err(SpoutError::ResponseTooLargeLimit(limit / (1024 * 1024)));
    }

    Ok(raw_body)
}

#[cfg(test)]
mod tests {
    use super::{format_clean_status, limit_error_body, read_limited_body};
    use crate::sanitize::SanitizeStatus;

    #[test]
    fn large_response_fails() {
        let body = vec![b'a'; 10 * 1024 * 1024 + 1];
        let err = read_limited_body(std::io::Cursor::new(body), 10 * 1024 * 1024).unwrap_err();
        assert_eq!(err.to_string(), "response exceeds 10 MB limit");
    }

    #[test]
    fn error_body_is_trimmed() {
        let body = "a".repeat(5000);
        let limited = limit_error_body(&body);
        assert_eq!(limited.len(), 4099);
        assert!(limited.ends_with("..."));
    }

    #[test]
    fn clean_status_is_consistent() {
        assert_eq!(format_clean_status(SanitizeStatus::Cleaned), "cleaned");
        assert_eq!(format_clean_status(SanitizeStatus::Kept), "kept");
        assert_eq!(format_clean_status(SanitizeStatus::Unknown), "unsupported");
        assert_eq!(format_clean_status(SanitizeStatus::Disabled), "off");
    }
}
