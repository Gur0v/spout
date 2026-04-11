# spout
the missing link between your screenshot tool and the internet.

![Showcase](assets/showcase.gif)

Most screenshot tools want to own your entire workflow — hotkey, capture, upload, all bundled together. spout doesn't care about any of that. It just reads bytes from stdin and gives you a URL. What produces those bytes is your problem. It's a pipe segment. That's it.

## Install
```sh
git clone https://github.com/Gur0v/spout
cd spout
sh ./install
```

To uninstall globally, just run:
```sh
sh ./uninstall
```

#### Arch Linux (AUR)
```sh
paru -S spout      # stable
paru -S spout-git  # git HEAD
```

## Configure
Run `spout -g` to generate the default configuration file at `~/.config/spout/config.kdl`.

Two standard profiles are included. `litterbox` is ephemeral — files expire after 24 hours. `catbox` is permanent. Pick based on whether you want the file to stick around. Examples for custom backends like EZ-Host and Zendesk are also included.

```kdl
default "litterbox"

// wl-copy for Wayland, or uncomment one of the X11 alternatives
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
```

By default, spout enforces safety limits to prevent SSRF, DNS rebinding, and accidental massive uploads (100MB cap). Clipboard interaction is restricted to standard binaries (`wl-copy`, `xclip`, `xsel`).

### Profile options
| Field | Description |
|---|---|
| `url` | Upload endpoint. `{filename}` gets replaced with the generated filename. |
| `method` | `POST` or `PUT` |
| `format` | `multipart` or `binary` |
| `file-field` | The multipart field name for the file (defaults to `file`) |
| `field` | Extra multipart fields, repeatable |
| `header` | Extra headers — auth tokens, content types, etc. Repeatable. |
| `path` | Dot-separated path to the URL in the JSON response. Use `"."` for plain-text responses. |
| `filename` | `prefix`, `random` (N random hex chars), `extension` — all optional |

### Expert mode
Adding `yolo true` to the root of your config disables SSRF protection, certificate validation, redirect limits, upload size caps, filename validation, and the clipboard binary allowlist. This is intended for non-standard environments where you control the entire stack and understand exactly what you're turning off. Don't use it otherwise.

## Use
```sh
# generate the default config before first use
spout -g

# pipe anything in, get a URL out
flameshot gui -r | spout

# use a specific profile
flameshot gui -r | spout catbox

# override the file extension
spout -x mp4 < video.mp4

# override the filename entirely
spout -n my-screenshot.png < image.png
```

URL goes to stdout. URL also goes to your clipboard. That's the whole program.

### Flags
| Flag | Description |
|---|---|
| `-g`, `--gen-config` | Generate the default config file in your config directory. |
| `-G`, `--gen-config-force` | Overwrite your existing config with the default. |
| `-p`, `--parse` | Parse and validate the config file, then exit. |
| `-n`, `--name NAME` | Override the uploaded filename entirely. |
| `-x`, `--ext EXT` | Override the file extension, ignoring the profile's default. |
| `-v`, `--version` | Print the version and exit. |
| `-h`, `--help` | Print usage and exit. |

## Status
Verified on Linux (Spectacle, Flameshot, Grim, Scrot), FreeBSD and OpenBSD. HTTP/1.1 is strictly enforced for compatibility with legacy backends.

Windows is out of scope — use [ShareX](https://getsharex.com/). macOS is currently unsupported.

## License
[GPL-3.0](LICENSE)
