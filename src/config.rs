use directories::ProjectDirs;
use knuffel::Decode;
use std::fs;
use std::io::Write;

use crate::error::{Result, SpoutError};

pub const DEFAULT_CONFIG: &str = r#"default "0x0"

clipboard "wl-copy"
// clipboard "xclip" "-selection" "clipboard"
// clipboard "xsel" "--clipboard" "--input"

profile "0x0" {
    url "https://0x0.st"
    method "POST"
    format "multipart"
    file-field "file"
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

profile "imgur" {
    url "https://api.imgur.com/3/upload"
    method "POST"
    format "multipart"
    file-field "image"
    header "Authorization" "Client-ID YOUR_CLIENT_ID_HERE"
    path "data.link"
}
"#;

#[derive(Decode, Debug)]
pub struct Config {
    #[knuffel(child, unwrap(argument))]
    pub default: String,
    #[knuffel(child, unwrap(arguments))]
    pub clipboard: Option<Vec<String>>,
    #[knuffel(children(name = "profile"))]
    pub profiles: Vec<Profile>,
    #[knuffel(child, unwrap(argument), default)]
    pub yolo: bool,
}

#[derive(Decode, Debug)]
pub struct Profile {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(child, unwrap(argument))]
    pub url: String,
    #[knuffel(child, unwrap(argument))]
    pub method: String,
    #[knuffel(child, unwrap(argument))]
    pub format: String,
    #[knuffel(child, unwrap(argument))]
    pub path: String,
    #[knuffel(child, unwrap(argument))]
    pub strip_meta: Option<bool>,
    #[knuffel(child)]
    pub filename: Option<FilenameConfig>,
    #[knuffel(child, unwrap(argument), default = "file".to_string())]
    pub file_field: String,
    #[knuffel(children(name = "header"))]
    pub headers: Vec<KeyValue>,
    #[knuffel(children(name = "field"))]
    pub fields: Vec<KeyValue>,
}

#[derive(Decode, Debug)]
pub struct FilenameConfig {
    #[knuffel(property)]
    pub prefix: Option<String>,
    #[knuffel(property)]
    pub random: Option<usize>,
    #[knuffel(property)]
    pub extension: Option<String>,
}

#[derive(Decode, Debug)]
pub struct KeyValue {
    #[knuffel(argument)]
    pub key: String,
    #[knuffel(argument)]
    pub value: String,
}

pub fn config_path() -> Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("", "", "spout").ok_or(SpoutError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.kdl"))
}

pub fn write_config(force: bool) -> Result<()> {
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

    let mut file = opts.open(&path)?;
    file.write_all(DEFAULT_CONFIG.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    eprintln!("spout: config written to {}", path.display());
    Ok(())
}

pub fn load_config() -> Result<Config> {
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

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn profile_strip_meta_defaults_to_none() {
        let cfg: Config = knuffel::parse(
            "config.kdl",
            r#"
default "x"
profile "x" {
    url "https://example.com"
    method "POST"
    format "multipart"
    path "."
}
"#,
        )
        .unwrap();

        assert_eq!(cfg.profiles[0].strip_meta, None);
    }

    #[test]
    fn profile_strip_meta_can_be_overridden() {
        let cfg: Config = knuffel::parse(
            "config.kdl",
            r#"
default "x"
profile "x" {
    url "https://example.com"
    method "POST"
    format "multipart"
    path "."
    strip-meta true
}
"#,
        )
        .unwrap();

        assert_eq!(cfg.profiles[0].strip_meta, Some(true));
    }
}
