use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use knuffel::Decode;
use lexopt::prelude::*;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{self, IsTerminal, Read, Write as IoWrite};
use std::net::{IpAddr, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::Duration;

const MAX_UPLOAD: u64 = 100 * 1024 * 1024;
const MAX_RESPONSE: u64 = 10 * 1024 * 1024;
const TIMEOUT_STDIN: u64 = 120;
const TIMEOUT_YOLO: u64 = 3600;
const TIMEOUT_HTTP: u64 = 30;
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
    #[knuffel(child, unwrap(argument))]
    file_field: Option<String>,
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
        let mut p = lexopt::Parser::from_env();
        let mut cli = Cli::default();

        while let Some(arg) = p.next()? {
            match arg {
                Short('h') | Long("help") => {
                    println!(
                        "Usage: <tool> | spout [PROFILE] [OPTIONS]\n\
                         Options:\n\
                         \x20 -p, --parse            Parse config for errors\n\
                         \x20 -n, --name <NAME>      Override filename\n\
                         \x20 -x, --ext <EXT>        Override file extension\n\
                         \x20 -g, --gen-config       Generate default config\n\
                         \x20 -G, --gen-config-force Overwrite config with default\n\
                         \x20 -h, --help             Show help\n\
                         \x20 -v, --version          Show version"
                    );
                    std::process::exit(0);
                }
                Short('v') | Long("version") => {
                    let commit = Command::new("git")
                    .args(["rev-parse", "--short", "HEAD"])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "unknown".to_string());

                    let branch = Command::new("git")
                    .args(["rev-parse", "--abbrev-ref", "HEAD"])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "detached".to_string());

                    let date = Command::new("git")
                    .args(["log", "-1", "--format=%cs"])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "unknown-date".to_string());

                    let dirty = Command::new("git")
                    .args(["status", "--porcelain"])
                    .output()
                    .map(|o| if o.stdout.is_empty() { "" } else { "-dirty" })
                    .unwrap_or("");

                    println!(
                        "spout v{} ({} on {}, {}){}",
                             env!("CARGO_PKG_VERSION"),
                             commit,
                             branch,
                             date,
                             dirty
                    );
                    std::process::exit(0);
                }
                Short('p') | Long("parse") => cli.check = true,
                Short('g') | Long("gen-config") => cli.gen_config = true,
                Short('G') | Long("gen-config-force") => cli.gen_config_force = true,
                Short('x') | Long("ext") => {
                    cli.ext = Some(
                        p.value()?
                            .into_string()
                            .map_err(|s| anyhow!("Invalid UTF-8 in --ext: {:?}", s))?,
                    );
                }
                Short('n') | Long("name") => {
                    cli.name = Some(
                        p.value()?
                            .into_string()
                            .map_err(|s| anyhow!("Invalid UTF-8 in --name: {:?}", s))?,
                    );
                }
                Value(v) if cli.profile.is_none() => {
                    cli.profile = Some(
                        v.into_string()
                            .map_err(|s| anyhow!("Invalid UTF-8 in profile: {:?}", s))?,
                    );
                }
                _ => return Err(arg.unexpected().into()),
            }
        }

        Ok(cli)
    }
}

fn config_path() -> Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("", "", "spout").context("Failed to resolve config dir")?;
    Ok(dirs.config_dir().join("config.kdl"))
}

fn write_config(force: bool) -> Result<()> {
    let path = config_path()?;

    if path.exists() && !force {
        return Err(anyhow!(
            "Config exists at {}. Use -G to overwrite.",
            path.display()
        ));
    }

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    opts.open(&path)?.write_all(DEFAULT_CONFIG.as_bytes())?;
    println!("Config written to {}", path.display());
    Ok(())
}

fn load_config() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        return Err(anyhow!(
            "Config not found at {}. Run `spout -g`.",
            path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&path) {
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(anyhow!(
                    "Insecure config permissions. Run: chmod 600 {}",
                    path.display()
                ));
            }
        }
    }

    let text = fs::read_to_string(&path)?;
    let path_str = path.to_str().unwrap_or("config.kdl");
    knuffel::parse(path_str, &text).map_err(|e| anyhow!("Parse error: {}", e))
}

fn generate_filename(
    cfg: Option<&FilenameConfig>,
    override_ext: Option<&str>,
    override_name: Option<&str>,
) -> Result<String> {
    if let Some(name) = override_name {
        let ext = override_ext.unwrap_or("");
        return Ok(if ext.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", name, ext.trim_start_matches('.'))
        });
    }

    let mut base = cfg.and_then(|c| c.prefix.clone()).unwrap_or_default();

    if let Some(n) = cfg.and_then(|c| c.random) {
        let byte_len = (n + 1) / 2;
        let mut buf = vec![0u8; byte_len];
        getrandom::fill(&mut buf).map_err(|e| anyhow!("RNG error: {}", e))?;
        base.push_str(&hex::encode(&buf)[..n]);
    }

    if base.is_empty() {
        base.push_str("ss");
    }

    let ext = override_ext
        .map(String::from)
        .or_else(|| cfg.and_then(|c| c.extension.clone()))
        .unwrap_or_else(|| "png".to_string());

    Ok(format!("{}.{}", base, ext.trim_start_matches('.')))
}

fn validate_filename(name: &str) -> Result<()> {
    if name.chars().any(|c| matches!(c, '/' | '\\' | '\r' | '\n' | '\0'))
        || name.contains("..")
        || name.to_ascii_lowercase().contains("%2f")
    {
        return Err(anyhow!("Dangerous characters in filename: {}", name));
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
            let s = v6.segments();
            v6.is_loopback() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

fn resolve_url(url: &str) -> Result<(reqwest::Url, std::net::SocketAddr)> {
    let parsed = reqwest::Url::parse(url).context("Invalid URL")?;

    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(anyhow!("Unsupported scheme: {}", s)),
    }

    let host = parsed.host_str().ok_or_else(|| anyhow!("No host in URL"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| anyhow!("No port in URL"))?;

    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .context("DNS resolution failed")?
        .collect();

    if addrs.is_empty() {
        return Err(anyhow!("No addresses resolved for host"));
    }

    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            return Err(anyhow!("URL resolves to a private IP address"));
        }
    }

    Ok((parsed, addrs[0]))
}

fn parse_url_yolo(url: &str) -> Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(url).context("Invalid URL")?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        s => Err(anyhow!("Unsupported scheme: {}", s)),
    }
}

fn extract_response_value(body: &str, path: &str) -> Result<String> {
    if path == "." {
        return Ok(body.trim().to_string());
    }

    let json: serde_json::Value = serde_json::from_str(body).context("Response is not valid JSON")?;

    let value = path
        .split('.')
        .try_fold(&json, |cur, key| {
            cur.get(key)
                .or_else(|| key.parse::<usize>().ok().and_then(|i| cur.get(i)))
                .ok_or(key)
        })
        .map_err(|k| anyhow!("Key '{}' not found in response", k))?;

    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Response path '{}' is not a string", path))
}

fn validate_response_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).context("Response value is not a valid URL")?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        s => Err(anyhow!("Unexpected scheme in response URL: {}", s)),
    }
}

fn run_clipboard(cmd: &[String], text: &str, yolo: bool) -> Result<()> {
    let (bin, args) = match cmd.split_first() {
        Some(x) => x,
        None => return Ok(()),
    };

    if !yolo {
        if bin.contains('/') || bin.contains('\\') {
            return Err(anyhow!("Clipboard binary must be a name, not a path"));
        }
        if !ALLOWED_CLIPBOARD_BINS.contains(&bin.as_str()) {
            return Err(anyhow!("Clipboard binary '{}' is not allowed", bin));
        }
    }

    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn clipboard binary '{}'", bin))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).ok();
    }

    let status = child.wait()?;
    if !status.success() {
        eprintln!("spout: clipboard command exited with: {}", status);
    }

    Ok(())
}

fn read_stdin(max_bytes: Option<u64>, timeout_secs: u64) -> Result<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = if let Some(max) = max_bytes {
            io::stdin().take(max + 1).read_to_end(&mut buf)
        } else {
            io::stdin().read_to_end(&mut buf)
        };
        let _ = tx.send((buf, result));
    });

    let (data, result) = rx
        .recv_timeout(Duration::from_secs(timeout_secs))
        .map_err(|_| anyhow!("Stdin read timed out after {}s", timeout_secs))?;

    result.context("Failed to read from stdin")?;

    if let Some(max) = max_bytes {
        if data.len() as u64 > max {
            return Err(anyhow!("Input exceeds maximum size of {} bytes", max));
        }
    }

    Ok(data)
}

fn build_request(
    client: &reqwest::blocking::Client,
    profile: &Profile,
    url: reqwest::Url,
    data: Vec<u8>,
    filename: &str,
) -> Result<reqwest::blocking::Response> {
    let mut req = match profile.method.to_ascii_uppercase().as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        m => return Err(anyhow!("Unsupported HTTP method: {}", m)),
    };

    for h in &profile.headers {
        req = req.header(&h.key, &h.value);
    }

    let response = if profile.format == "multipart" {
        let mime = mime_guess::from_path(filename).first_or_octet_stream();
        let field_name = profile
            .file_field
            .clone()
            .unwrap_or_else(|| "file".to_string());

        let part = reqwest::blocking::multipart::Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str(mime.as_ref())?;

        let mut form = reqwest::blocking::multipart::Form::new();
        for f in &profile.fields {
            form = form.text(f.key.clone(), f.value.clone());
        }

        req.multipart(form.part(field_name, part)).send()?
    } else {
        req.body(data).send()?
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
        println!("Config OK");
        return Ok(());
    }

    if io::stdin().is_terminal() {
        return Err(anyhow!("No input data. Usage: <cmd> | spout [profile]"));
    }

    let config = load_config()?;
    let target = cli.profile.unwrap_or_else(|| config.default.clone());

    let profile = config
        .profiles
        .into_iter()
        .find(|p| p.name == target)
        .ok_or_else(|| anyhow!("No profile named '{}'", target))?;

    let filename = generate_filename(
        profile.filename.as_ref(),
        cli.ext.as_deref(),
        cli.name.as_deref(),
    )?;

    if !config.yolo {
        validate_filename(&filename)?;
    }

    let raw_url = profile.url.replace("{filename}", &uri_encode(&filename));

    let data = if config.yolo {
        read_stdin(None, TIMEOUT_YOLO)?
    } else {
        read_stdin(Some(MAX_UPLOAD), TIMEOUT_STDIN)?
    };

    let mut builder = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .http1_only()
        .user_agent(concat!("spout/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(TIMEOUT_HTTP));

    let url = if config.yolo {
        builder = builder
            .redirect(reqwest::redirect::Policy::limited(10))
            .danger_accept_invalid_certs(true);
        parse_url_yolo(&raw_url)?
    } else {
        let (parsed, addr) = resolve_url(&raw_url)?;
        builder = builder
            .redirect(reqwest::redirect::Policy::none())
            .resolve(parsed.host_str().unwrap_or(""), addr);
        parsed
    };

    let client = builder.build()?;
    let mut response = build_request(&client, &profile, url, data, &filename)?;

    let status = response.status();
    let mut raw_body = Vec::new();

    if config.yolo {
        response.read_to_end(&mut raw_body).context("Failed to read response")?;
    } else {
        response
            .take(MAX_RESPONSE)
            .read_to_end(&mut raw_body)
            .context("Failed to read response")?;
    }

    let body = String::from_utf8_lossy(&raw_body);

    if !status.is_success() {
        return Err(anyhow!("Upload failed ({}): {}", status, body.trim()));
    }

    let result = extract_response_value(&body, &profile.path)?;

    if !config.yolo {
        validate_response_url(&result)?;
    }

    println!("{}", result);

    if let Some(cmd) = config.clipboard {
        if let Err(e) = run_clipboard(&cmd, &result, config.yolo) {
            eprintln!("spout: clipboard error: {}", e);
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    run()
}
