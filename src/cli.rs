use std::env;
use std::ffi::OsString;

use crate::error::{Result, SpoutError};

#[derive(Debug, Default)]
pub struct Cli {
    pub profile: Option<String>,
    pub name: Option<String>,
    pub ext: Option<String>,
    pub no_clipboard: bool,
    pub check: bool,
    pub gen_config: bool,
    pub gen_config_force: bool,
}

impl Cli {
    pub fn parse() -> Result<Self> {
        let mut cli = Cli::default();
        let mut args = env::args_os().skip(1);

        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("-h" | "--help") => {
                    println!(
                        "usage: <cmd> | spout [profile] [options]\n\
                         \n\
                         options:\n\
                         \x20 -p, --parse              parse config for errors\n\
                         \x20 -n, --name <name>        override filename\n\
                         \x20 -N, --no-clipboard       skip clipboard copy\n\
                         \x20 -x, --ext <ext>          override file extension\n\
                         \x20 -g, --gen-config         generate default config\n\
                         \x20 -G, --gen-config-force   overwrite config with default\n\
                         \x20 -h, --help               show this help\n\
                         \x20 -v, --version            show version"
                    );
                    std::process::exit(0);
                }
                Some("-v" | "--version") => {
                    println!("spout v{}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                Some("-p" | "--parse") => cli.check = true,
                Some("-N" | "--no-clipboard") => cli.no_clipboard = true,
                Some("-g" | "--gen-config") => cli.gen_config = true,
                Some("-G" | "--gen-config-force") => cli.gen_config_force = true,
                Some("-x" | "--ext") => cli.ext = Some(value(&mut args, "--ext")?),
                Some("-n" | "--name") => cli.name = Some(value(&mut args, "--name")?),
                Some(s) if let Some(v) = s.strip_prefix("--ext=") => cli.ext = Some(v.to_string()),
                Some(s) if let Some(v) = s.strip_prefix("--name=") => {
                    cli.name = Some(v.to_string());
                }
                Some(s) if s.starts_with('-') => {
                    return Err(SpoutError::InvalidArgument(format!(
                        "unexpected argument: {s}"
                    )));
                }
                _ if cli.profile.is_none() => {
                    cli.profile = Some(
                        arg.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("profile name", s))?,
                    );
                }
                _ => {
                    return Err(SpoutError::InvalidArgument(format!(
                        "unexpected argument: {:?}",
                        arg
                    )));
                }
            }
        }

        Ok(cli)
    }
}

fn value(args: &mut impl Iterator<Item = OsString>, name: &'static str) -> Result<String> {
    args.next()
        .ok_or_else(|| SpoutError::InvalidArgument(format!("missing value for {name}")))?
        .into_string()
        .map_err(|s| SpoutError::InvalidUtf8(name, s))
}
