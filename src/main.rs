use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use knuffel::Decode;
use lexopt::prelude::*;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

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
    headers: Vec<KV>,
    #[knuffel(children(name = "field"))]
    fields: Vec<KV>,
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
struct KV {
    #[knuffel(argument)]
    key: String,
    #[knuffel(argument)]
    value: String,
}

struct Cli {
    profile: Option<String>,
    name: Option<String>,
    ext: Option<String>,
    check: bool,
    gen_config: bool,
    gen_config_force: bool,
}

fn parse_cli() -> Result<Cli> {
    let mut parser = lexopt::Parser::from_env();
    let mut cli = Cli {
        profile: None,
        name: None,
        ext: None,
        check: false,
        gen_config: false,
        gen_config_force: false,
    };

    while let Some(arg) = parser.next()? {
        match arg {
            Short('h') | Long("help") => {
                println!("Usage: <tool> | spout [PROFILE] [OPTIONS]\nOptions:\n  -p, --parse            Parse config for errors\n  -n, --name <NAME>      Override filename\n  -x, --ext <EXT>        Override file extension\n  -g, --gen-config       Generate default config\n  -G, --gen-config-force Overwrite config with default\n  -h, --help             Show help\n  -v, --version          Show version");
                std::process::exit(0);
            }
            Short('v') | Long("version") => {
                println!("spout v{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Short('p') | Long("parse") => cli.check = true,
            Short('g') | Long("gen-config") => cli.gen_config = true,
            Short('G') | Long("gen-config-force") => cli.gen_config_force = true,
            Short('x') | Long("ext") => {
                cli.ext = Some(parser.value()?.into_string().map_err(|s| anyhow!("Invalid UTF-8 in argument: {:?}", s))?)
            }
            Short('n') | Long("name") => {
                cli.name = Some(parser.value()?.into_string().map_err(|s| anyhow!("Invalid UTF-8 in argument: {:?}", s))?)
            }
            Value(val) if cli.profile.is_none() => {
                cli.profile = Some(val.into_string().map_err(|s| anyhow!("Invalid UTF-8 in argument: {:?}", s))?)
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    Ok(cli)
}

fn write_default_config(force: bool) -> Result<()> {
    let proj_dirs = ProjectDirs::from("", "", "spout").context("Failed to resolve config dir")?;
    let config_dir = proj_dirs.config_dir();
    let config_path = config_dir.join("config.kdl");

    if config_path.exists() && !force {
        return Err(anyhow!(
            "Config already exists at {}. Use -G to overwrite.",
            config_path.display()
        ));
    }

    fs::create_dir_all(config_dir)?;
    fs::write(&config_path, DEFAULT_CONFIG)?;
    println!("Config written to {}", config_path.display());
    Ok(())
}

fn load_config() -> Result<Config> {
    let proj_dirs = ProjectDirs::from("", "", "spout").context("Failed to resolve config dir")?;
    let config_path = proj_dirs.config_dir().join("config.kdl");

    if !config_path.exists() {
        return Err(anyhow!(
            "Config not found at {}. Run `spout -g` to create one.",
            config_path.display()
        ));
    }

    let config_text = fs::read_to_string(&config_path)?;
    let path_str = config_path.to_str().unwrap_or("config.kdl");
    knuffel::parse(path_str, &config_text).map_err(|e| anyhow!("Config parse error: {}", e))
}

fn generate_filename(
    cfg: Option<&FilenameConfig>,
    ext_override: Option<&str>,
    name_override: Option<&str>,
) -> Result<String> {
    if let Some(n) = name_override {
        return Ok(match ext_override {
            Some(e) => format!("{}.{}", n, e.trim_start_matches('.')),
            None => n.to_string(),
        });
    }

    let mut name = cfg.and_then(|c| c.prefix.clone()).unwrap_or_default();
    
    if let Some(n) = cfg.and_then(|c| c.random) {
        let mut buf = vec![0u8; n];
        getrandom::getrandom(&mut buf).map_err(|e| anyhow!("RNG failure: {}", e))?;
        name.push_str(&hex::encode(&buf));
    }
    
    if name.is_empty() {
        name.push_str("ss");
    }

    let ext = ext_override
        .map(String::from)
        .or_else(|| cfg.and_then(|c| c.extension.clone()))
        .unwrap_or_else(|| "png".to_string());

    Ok(format!("{}.{}", name, ext.trim_start_matches('.')))
}

fn extract_url(raw: &str, path: &str) -> Result<String> {
    if path == "." {
        return Ok(raw.trim().to_string());
    }

    let json: serde_json::Value = serde_json::from_str(raw).context("Invalid JSON response")?;

    path.split('.')
        .try_fold(&json, |cur, key| {
            cur.get(key)
                .or_else(|| key.parse::<usize>().ok().and_then(|idx| cur.get(idx)))
                .ok_or(key)
        })
        .map_err(|key| anyhow!("Key '{}' not found in JSON", key))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Value at path '{}' is not a string", path))
}

fn execute_clipboard(cmd_args: &[String], text: &str) -> Result<()> {
    if let Some((bin, args)) = cmd_args.split_first() {
        let mut child = Command::new(bin)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn clipboard command")?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }

        let status = child.wait()?;
        if !status.success() {
            eprintln!("spout: clipboard command failed with status {}", status);
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = parse_cli()?;

    if cli.gen_config_force {
        return write_default_config(true);
    }

    if cli.gen_config {
        return write_default_config(false);
    }

    if cli.check {
        load_config()?;
        println!("Config OK");
        return Ok(());
    }

    if io::stdin().is_terminal() {
        return Err(anyhow!("No data piped to stdin. Usage: <tool> | spout"));
    }

    let config = load_config()?;
    let target = cli.profile.unwrap_or(config.default);

    let profile = config
        .profiles
        .into_iter()
        .find(|p| p.name == target)
        .ok_or_else(|| anyhow!("Profile '{}' not found", target))?;

    let filename = generate_filename(
        profile.filename.as_ref(),
        cli.ext.as_deref(),
        cli.name.as_deref(),
    )?;
    
    let url = profile.url.replace("{filename}", &filename);

    let client = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .http1_only()
        .user_agent("spout")
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut req = match profile.method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        m => return Err(anyhow!("Unsupported HTTP method: {}", m)),
    };

    for h in profile.headers {
        req = req.header(&h.key, &h.value);
    }

    let response = if profile.format == "multipart" {
        let mut data = Vec::new();
        io::stdin().read_to_end(&mut data).context("Failed to read stdin")?;
        
        let mut form = reqwest::blocking::multipart::Form::new();
        for f in profile.fields {
            form = form.text(f.key, f.value);
        }

        let mime = mime_guess::from_path(&filename).first_or_octet_stream();
        let field_name = profile.file_field.unwrap_or_else(|| "file".to_string());
        
        let part = reqwest::blocking::multipart::Part::bytes(data)
            .file_name(filename)
            .mime_str(mime.as_ref())?;

        req.multipart(form.part(field_name, part)).send()?
    } else {
        req.body(reqwest::blocking::Body::new(io::stdin())).send()?
    };

    let status = response.status();
    let body = response.text().unwrap_or_default();
    
    if !status.is_success() {
        return Err(anyhow!("Upload failed ({}): {}", status, body));
    }

    let result_url = extract_url(&body, &profile.path)?;
    println!("{result_url}");

    if let Some(cmd) = config.clipboard {
        let _ = execute_clipboard(&cmd, &result_url);
    }

    Ok(())
}
