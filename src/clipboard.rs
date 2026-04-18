use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{Result, SpoutError};

const ALLOWED_CLIPBOARD_BINS: &[&str] = &["wl-copy", "xclip", "xsel"];

pub fn run_clipboard(cmd: &[String], text: &str, yolo: bool) -> Result<()> {
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
