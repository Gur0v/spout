use lexopt::prelude::*;

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
                         \x20 -N, --no-clipboard       skip clipboard copy\n\
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
                Short('N') | Long("no-clipboard") => cli.no_clipboard = true,
                Short('g') | Long("gen-config") => cli.gen_config = true,
                Short('G') | Long("gen-config-force") => cli.gen_config_force = true,
                Short('x') | Long("ext") => {
                    let raw = parser.value()?;
                    cli.ext = Some(
                        raw.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("--ext", s))?,
                    );
                }
                Short('n') | Long("name") => {
                    let raw = parser.value()?;
                    cli.name = Some(
                        raw.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("--name", s))?,
                    );
                }
                Value(raw) if cli.profile.is_none() => {
                    cli.profile = Some(
                        raw.into_string()
                            .map_err(|s| SpoutError::InvalidUtf8("profile name", s))?,
                    );
                }
                _ => return Err(arg.unexpected().into()),
            }
        }

        Ok(cli)
    }
}
