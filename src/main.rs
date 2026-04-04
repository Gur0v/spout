use anyhow::{anyhow, Context, Result};
use knuffel::Decode;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

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

fn generate_filename(
    cfg: &Option<FilenameConfig>,
    ext_override: Option<&String>,
    name_override: Option<&String>,
) -> String {
    if let Some(n) = name_override {
        if let Some(e) = ext_override {
            let mut s = n.clone();
            if !e.starts_with('.') {
                s.push('.');
            }
            s.push_str(e);
            return s;
        }
        return n.clone();
    }

    let mut name = String::new();
    let mut ext = ext_override.map(|s| s.as_str());

    if let Some(c) = cfg {
        name = c.prefix.clone().unwrap_or_default();
        if let Some(n) = c.random {
            let mut buf = vec![0u8; n];
            getrandom::fill(&mut buf).expect("getrandom failed");
            name.push_str(&hex::encode(&buf));
        }
        if ext.is_none() {
            ext = c.extension.as_deref();
        }
    }

    if name.is_empty() {
        name.push_str("ss");
    }

    let final_ext = ext.unwrap_or("png");
    if !final_ext.starts_with('.') {
        name.push('.');
    }
    name.push_str(final_ext);

    name
}

fn extract_url(raw: &str, path: &str) -> Result<String> {
    if path == "." {
        return Ok(raw.trim().to_string());
    }

    let json: serde_json::Value =
        serde_json::from_str(raw).context("Failed to parse server response as JSON")?;

    path.split('.')
        .try_fold(&json, |cur, key| {
            cur.get(key)
                .or_else(|| key.parse::<usize>().ok().and_then(|idx| cur.get(idx)))
                .ok_or(key)
        })
        .map_err(|key| anyhow!("Key '{}' not found in JSON response", key))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Value at path '{}' is not a string", path))
}

fn load_config() -> Result<Config> {
    let config_dir = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").expect("$HOME not set");
        format!("{}/.config", home)
    });

    let config_path = format!("{}/spout/config.kdl", config_dir);
    let config_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Missing config file: {}", config_path))?;
    knuffel::parse(&config_path, &config_text)
        .map_err(|e| anyhow!("Config parse error: {}", e))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut profile_name = None;
    let mut ext_override = None;
    let mut name_override = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" => {
                println!("Usage: <tool> | spout [PROFILE] [-n NAME] [-x EXT]\nFlags:\n  -p  Parse config for errors\n  -n  Override filename\n  -x  Override file extension\n  -h  Show help\n  -v  Show version");
                return Ok(());
            }
            "-v" => {
                println!("spout v{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-p" => {
                load_config()?;
                println!("Config OK");
                return Ok(());
            }
            "-x" => {
                ext_override = args.next();
                if ext_override.is_none() {
                    return Err(anyhow!("Missing argument for -x"));
                }
            }
            "-n" => {
                name_override = args.next();
                if name_override.is_none() {
                    return Err(anyhow!("Missing argument for -n"));
                }
            }
            name => {
                if profile_name.is_none() {
                    profile_name = Some(name.to_string());
                }
            }
        }
    }

    let mut data = Vec::new();
    std::io::stdin()
        .read_to_end(&mut data)
        .context("Failed to read from stdin")?;

    if data.is_empty() {
        return Err(anyhow!("No data received. Usage: <tool> | spout"));
    }

    let config = load_config()?;
    let target_profile = profile_name.unwrap_or(config.default);

    let profile = config
        .profiles
        .iter()
        .find(|p| p.name == target_profile)
        .ok_or_else(|| anyhow!("Profile '{}' not found", target_profile))?;

    let filename = generate_filename(&profile.filename, ext_override.as_ref(), name_override.as_ref());
    let url = profile.url.replace("{filename}", &filename);

    let client = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .http1_only()
        .user_agent("spout")
        .build()?;

    let mut req = match profile.method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        m => return Err(anyhow!("Unsupported HTTP method: {}", m)),
    };

    for h in &profile.headers {
        req = req.header(&h.key, &h.value);
    }

    let response = if profile.format == "multipart" {
        let mut form = reqwest::blocking::multipart::Form::new();
        for f in &profile.fields {
            form = form.text(f.key.clone(), f.value.clone());
        }

        let mime_type = mime_guess::from_path(&filename).first_or_octet_stream();

        let field_name = profile.file_field.clone().unwrap_or_else(|| "file".to_string());
        let part = reqwest::blocking::multipart::Part::bytes(data)
            .file_name(filename)
            .mime_str(mime_type.as_ref())?;
        req.multipart(form.part(field_name, part)).send()?
    } else {
        req.body(data).send()?
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(anyhow!("Upload failed ({}): {}", status, body));
    }

    let result_url = extract_url(&response.text()?, &profile.path)?;
    println!("{result_url}");

    if let Some(args) = &config.clipboard {
        if let Some((bin, rest)) = args.split_first() {
            let mut child = Command::new(bin)
                .args(rest)
                .stdin(Stdio::piped())
                .spawn()
                .context("Failed to spawn clipboard command")?;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(result_url.as_bytes());
            }
            let _ = child.wait();
        }
    }

    Ok(())
}
