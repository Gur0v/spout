use directories::ProjectDirs;
use knuffel::Decode;
use lexopt::prelude::*;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{self, IsTerminal, Read, Write as IoWrite};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use thiserror::Error;

#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Error, Debug)]
pub enum SpoutError {
    #[error("invalid utf-8 in {0}: {1:?}")]
    InvalidUtf8(&'static str, std::ffi::OsString),

    #[error("failed to resolve config dir")]
    NoConfigDir,

    #[error("config exists at {0} -- use -G to overwrite")]
    ConfigExists(std::path::PathBuf),

    #[error("config not found at {0} -- run: spout -g")]
    ConfigNotFound(std::path::PathBuf),

    #[error("insecure config permissions -- run: chmod 600 {0}")]
    InsecureConfig(std::path::PathBuf),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("rng error: {0}")]
    RngError(#[source] getrandom::Error),

    #[error("dangerous characters in filename: {0}")]
    DangerousFilename(String),

    #[error("invalid url: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),

    #[error("no host in url")]
    NoHost,

    #[error("no port in url")]
    NoPort,

    #[error("dns resolution failed: {0}")]
    DnsResolution(#[source] io::Error),

    #[error("dns resolution timed out")]
    DnsTimeout,

    #[error("no addresses resolved for host")]
    NoAddresses,

    #[error("url resolves to a private ip address")]
    PrivateIp,

    #[error("response is not valid json: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("key '{0}' not found in response")]
    KeyNotFound(String),

    #[error("response path '{0}' is not a string")]
    NotAString(String),

    #[error("response body is too large to be a valid url")]
    ResponseTooLarge,

    #[error("response value is not a valid url: {0}")]
    ResponseInvalidUrl(#[source] url::ParseError),

    #[error("unexpected scheme in response url: {0}")]
    ResponseUnexpectedScheme(String),

    #[error("fields are not supported for binary format uploads")]
    FieldsInBinaryFormat,

    #[error("clipboard binary must be a name, not a path")]
    ClipboardPathNotAllowed,

    #[error("clipboard binary '{0}' is not allowed")]
    ClipboardBinaryNotAllowed(String),

    #[error("failed to spawn clipboard binary '{0}': {1}")]
    ClipboardSpawn(String, #[source] io::Error),

    #[error("failed to read from stdin: {0}")]
    StdinRead(#[source] io::Error),

    #[error("input exceeds {0} MB limit")]
    InputTooLarge(u64),

    #[error("unsupported http method: {0}")]
    UnsupportedMethod(String),

    #[error("no input data -- usage: <cmd> | spout [profile]")]
    NoInputData,

    #[error("no profile named '{0}'")]
    ProfileNotFound(String),

    #[error("failed to read response: {0}")]
    ResponseReadError(#[source] io::Error),

    #[error("upload failed ({0}) -- {1}")]
    UploadFailed(reqwest::StatusCode, String),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    Lexopt(#[from] lexopt::Error),
}

pub type Result<T, E = SpoutError> = std::result::Result<T, E>;

const MAX_UPLOAD: u64 = 100 * 1024 * 1024;
const MAX_RESPONSE: u64 = 10 * 1024 * 1024;
const MAX_URL_LEN: usize = 2048;
const HTTP_TIMEOUT_SECS: u64 = 30;
const DNS_TIMEOUT_MS: u64 = 1500;

const ALLOWED_CLIPBOARD_BINS: &[&str] = &["wl-copy", "xclip", "xsel"];

const DEFAULT_CONFIG: &str = r#"default "litterbox"

clipboard "wl-copy"
// clipboard "xclip" "-selection" "clipboard"
// clipboard "xsel" "--clipboard" "--input"

profile "litterbox" {
    url "https://litterbox.catbox.moe/resources/internals/api.php"
    method "POST"
    format "multipart"
    file-field "fileToUpload"
    field "reqtype" "fileupload"
    field "time" "24h"
    path "."
    filename random=8 extension="png"
}

profile "catbox" {
    url "https://catbox.moe/user/api.php"
    method "POST"
    format "multipart"
    file-field "fileToUpload"
    field "reqtype" "fileupload"
    path "."
    filename random=8 extension="png"
}

profile "zendesk" {
    url "https://support.zendesk.com/api/v2/uploads.json?filename={filename}"
    method "POST"
    format "binary"
    header "Content-Type" "application/octet-stream"
    path "upload.attachment.mapped_content_url"
    filename prefix="spout_" random=8 extension="png"
}

profile "ez" {
    url "https://api.e-z.host/files"
    method "POST"
    format "multipart"
    file-field "file"
    header "key" "YOUR_API_KEY_HERE"
    path "imageUrl"
    filename random=8 extension="png"
}
"#;

#[derive(Decode, Debug)]
struct Config {
    #[knuffel(child, unwrap(argument))]
    default: String,
    #[knuffel(child, unwrap(arguments))]
    clipboard: Option<Vec<String>>,
    #[knuffel(children(name = "profile"))]
    profiles: Vec<Profile>,
    #[knuffel(child, unwrap(argument), default)]
    yolo: bool,
}

#[derive(Decode, Debug)]
struct Profile {
    #[knuffel(argument)]
    name: String,
    #[knuffel(child, unwrap(argument))]
    url: String,
    #[knuffel(child, unwrap(argument))]
    method: String,
    #[knuffel(child, unwrap(argument))]
    format: String,
    #[knuffel(child, unwrap(argument))]
    path: String,
    #[knuffel(child)]
    filename: Option<FilenameConfig>,
    #[knuffel(child, unwrap(argument), default = "file".to_string())]
    file_field: String,
    #[knuffel(children(name = "header"))]
    headers: Vec<KeyValue>,
    #[knuffel(children(name = "field"))]
    fields: Vec<KeyValue>,
}

#[derive(Decode, Debug)]
struct FilenameConfig {
    #[knuffel(property)]
    prefix: Option<String>,
    #[knuffel(property)]
    random: Option<usize>,
    #[knuffel(property)]
    extension: Option<String>,
}

#[derive(Decode, Debug)]
struct KeyValue {
    #[knuffel(argument)]
    key: String,
    #[knuffel(argument)]
    value: String,
}

#[derive(Debug, Default)]
struct Cli {
    profile: Option<String>,
    name: Option<String>,
    ext: Option<String>,
    check: bool,
    gen_config: bool,
    gen_config_force: bool,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut parser = lexopt::Parser::from_env();
        let mut cli = Cli::default();

        while let Some(arg) = parser.next()? {
            match arg {
                Short('h') | Long("help") => {
                    println!(
                        "usage: <cmd> | spout [profile] [options]\n\
                         \n\
                         options:\n\
                         \x20 -p, --parse              parse config for errors\n\
                         \x20 -n, --name <name>        override filename\n\
                         \x20 -x, --ext <ext>          override file extension\n\
                         \x20 -g, --gen-config         generate default config\n\
                         \x20 -G, --gen-config-force   overwrite config with default\n\
                         \x20 -h, --help               show this help\n\
                         \x20 -v, --version            show version"
                    );
                    std::process::exit(0);
                }
                Short('v') | Long("version") => {
                    println!(
                        "spout v{} ({} on {}, {})",
                        env!("CARGO_PKG_VERSION"),
                        env!("VERGEN_GIT_SHA"),
                        env!("VERGEN_GIT_BRANCH"),
                        env!("VERGEN_GIT_COMMIT_DATE")
                    );
                    std::process::exit(0);
                }
                Short('p') | Long("parse") => cli.check = true,
                Short('g') | Long("gen-config") => cli.gen_config = true,
                Short('G') | Long("gen-config-force") => cli.gen_config_force = true,
                Short('x') | Long("ext") => {
                    let raw = parser.value()?;
                    cli.ext = Some(
                        raw.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("--ext", s))?
                    );
                }
                Short('n') | Long("name") => {
                    let raw = parser.value()?;
                    cli.name = Some(
                        raw.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("--name", s))?
                    );
                }
                Value(raw) if cli.profile.is_none() => {
                    cli.profile = Some(
                        raw.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("profile name", s))?
                    );
                }
                _ => return Err(arg.unexpected().into()),
            }
        }

        Ok(cli)
    }
}

fn config_path() -> Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("", "", "spout").ok_or(SpoutError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.kdl"))
}

fn write_config(force: bool) -> Result<()> {
    let path = config_path()?;

    if path.exists() && !force {
        return Err(SpoutError::ConfigExists(path));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    opts.open(&path)?.write_all(DEFAULT_CONFIG.as_bytes())?;
    eprintln!("spout: config written to {}", path.display());
    Ok(())
}

fn load_config() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        return Err(SpoutError::ConfigNotFound(path));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&path) {
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(SpoutError::InsecureConfig(path));
            }
        }
    }

    let text = fs::read_to_string(&path)?;
    let path_str = path.to_str().unwrap_or("config.kdl");
    knuffel::parse(path_str, &text).map_err(|e| SpoutError::ParseError(e.to_string()))
}

fn generate_filename(
    cfg: Option<&FilenameConfig>,
    ext_override: Option<&str>,
    name_override: Option<&str>,
) -> Result<String> {
    if let Some(name) = name_override {
        let ext = ext_override.unwrap_or("");
        return Ok(if ext.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", name, ext.trim_start_matches('.'))
        });
    }

    let mut stem = cfg.and_then(|c| c.prefix.clone()).unwrap_or_default();

    if let Some(n) = cfg.and_then(|c| c.random) {
        let byte_len = (n + 1) / 2;
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

fn validate_filename(name: &str) -> Result<()> {
    if name.chars().any(|c| matches!(c, '/' | '\\' | '\r' | '\n' | '\0'))
        || name.contains("..")
        || name.to_ascii_lowercase().contains("%2f")
    {
        return Err(SpoutError::DangerousFilename(name.to_string()));
    }
    Ok(())
}

fn uri_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127
                || o[0] == 10
                || o[0] == 0
                || (o[0] == 169 && o[1] == 254)
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 100 && (64..=127).contains(&o[1]))
        }
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            v6.is_loopback()
                || (segs[0] & 0xfe00) == 0xfc00
                || (segs[0] & 0xffc0) == 0xfe80
        }
    }
}

fn resolve_url(raw: &str) -> Result<(reqwest::Url, SocketAddr)> {
    let url = reqwest::Url::parse(raw)?;

    match url.scheme() {
        "http" | "https" => {}
        s => return Err(SpoutError::UnsupportedScheme(s.to_string())),
    }

    let host = url.host_str().ok_or(SpoutError::NoHost)?;
    let port = url.port_or_known_default().ok_or(SpoutError::NoPort)?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(SpoutError::PrivateIp);
        }
        return Ok((url, SocketAddr::new(ip, port)));
    }

    let host_port = format!("{}:{}", host, port);
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = host_port.to_socket_addrs();
        let _ = tx.send(result);
    });

    let addrs: Vec<SocketAddr> = rx
        .recv_timeout(Duration::from_millis(DNS_TIMEOUT_MS))
        .map_err(|_| SpoutError::DnsTimeout)?
        .map_err(SpoutError::DnsResolution)?
        .collect();

    if addrs.is_empty() {
        return Err(SpoutError::NoAddresses);
    }

    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            return Err(SpoutError::PrivateIp);
        }
    }

    Ok((url, addrs[0]))
}

fn parse_url_yolo(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        s => Err(SpoutError::UnsupportedScheme(s.to_string())),
    }
}

fn extract_response_value(body: &str, path: &str) -> Result<String> {
    if path == "." {
        return Ok(body.trim().to_string());
    }

    let json: serde_json::Value = serde_json::from_str(body)?;

    let node = path
        .split('.')
        .try_fold(&json, |cur, key| {
            cur.get(key)
                .or_else(|| key.parse::<usize>().ok().and_then(|i| cur.get(i)))
                .ok_or(key)
        })
        .map_err(|k| SpoutError::KeyNotFound(k.to_string()))?;

    node.as_str()
        .map(str::to_string)
        .ok_or_else(|| SpoutError::NotAString(path.to_string()))
}

fn validate_response_url(value: &str) -> Result<()> {
    if value.len() > MAX_URL_LEN {
        return Err(SpoutError::ResponseTooLarge);
    }
    let url = reqwest::Url::parse(value).map_err(SpoutError::ResponseInvalidUrl)?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        s => Err(SpoutError::ResponseUnexpectedScheme(s.to_string())),
    }
}

fn run_clipboard(cmd: &[String], text: &str, yolo: bool) -> Result<()> {
    let (bin, args) = match cmd.split_first() {
        Some(x) => x,
        None => return Ok(()),
    };

    if !yolo {
        if bin.contains('/') || bin.contains('\\') {
            return Err(SpoutError::ClipboardPathNotAllowed);
        }
        if !ALLOWED_CLIPBOARD_BINS.contains(&bin.as_str()) {
            return Err(SpoutError::ClipboardBinaryNotAllowed(bin.to_string()));
        }
    }

    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| SpoutError::ClipboardSpawn(bin.to_string(), e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).ok();
    }

    let status = child.wait()?;
    if !status.success() {
        eprintln!("spout: warn: clipboard exited with status {}", status);
    }

    Ok(())
}

fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

struct CountingReader<R> {
    inner: R,
    tally: Arc<AtomicUsize>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.tally.fetch_add(n, Ordering::Relaxed);
        Ok(n)
    }
}

fn send_request<R: Read + Send + 'static>(
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
        fmt => return Err(SpoutError::UnsupportedMethod(fmt.to_string())),
    };

    Ok(response)
}

fn run() -> Result<()> {
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
        parse_url_yolo(&raw_url)?
    } else {
        let (parsed, addr) = resolve_url(&raw_url)?;
        builder = builder
            .resolve(parsed.host_str().unwrap_or(""), addr)
            .redirect(reqwest::redirect::Policy::none());
        parsed
    };

    let client = builder.build()?;
    let tally = Arc::new(AtomicUsize::new(0));
    let stdin = io::stdin();

    let t0 = std::time::Instant::now();

    let mut response = if config.yolo {
        send_request(&client, &profile, url, CountingReader { inner: stdin, tally: Arc::clone(&tally) }, &filename)?
    } else {
        send_request(&client, &profile, url, CountingReader { inner: stdin.take(MAX_UPLOAD), tally: Arc::clone(&tally) }, &filename)?
    };

    let elapsed = t0.elapsed().as_secs_f64();
    let uploaded = tally.load(Ordering::Relaxed);
    let status = response.status();

    let mut raw_body = Vec::new();
    if config.yolo {
        response.read_to_end(&mut raw_body).map_err(SpoutError::ResponseReadError)?;
    } else {
        response.take(MAX_RESPONSE).read_to_end(&mut raw_body).map_err(SpoutError::ResponseReadError)?;
    }

    let body = String::from_utf8_lossy(&raw_body);

    if !status.is_success() {
        return Err(SpoutError::UploadFailed(status, body.trim().to_string()));
    }

    let result = extract_response_value(&body, &profile.path)?;

    if !config.yolo {
        validate_response_url(&result)?;
    }

    eprintln!(
        "spout: ok [{}] {} {} {:.1}s",
        target,
        filename,
        format_size(uploaded),
        elapsed
    );
    println!("{}", result);

    if let Some(cmd) = config.clipboard {
        if !cmd.is_empty() {
            if let Err(e) = run_clipboard(&cmd, &result, config.yolo) {
                eprintln!("spout: warn: clipboard error: {}", e);
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
