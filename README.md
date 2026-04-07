# spout
the missing link between your screenshot tool and the internet.

![Showcase](assets/showcase.gif)

Most screenshot tools want to own your entire workflow — hotkey, capture, upload, all bundled together. spout doesn't care about any of that. It just reads bytes from stdin and gives you a URL. What produces those bytes is your problem.

It's a pipe segment. That's it.

## Install

```sh
git clone https://github.com/Gur0v/spout
cd spout
cargo build --release
cp target/release/spout ~/.local/bin/
```

#### Arch Linux (AUR)

```sh
paru -S spout      # stable
paru -S spout-git  # git HEAD
```

## Configure

> [!NOTE]  
> Run `spout -g` to generate the default configuration file at `~/.config/spout/config.kdl` (or your OS's standard config directory).

The config format is [KDL](https://kdl.dev). It's like JSON but for humans.

Two standard profiles are included in the generated config. `litterbox` is ephemeral — files expire after 24 hours. `catbox` is permanent. Pick whichever matches your threat level. *(Note: Examples for custom API backends like EZ-Host and Zendesk are also included in the generated config).*

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
| `filename` | `prefix`, `random` (N random hex bytes), `extension` — all optional |

If you're pointing spout at your own backend, `header` is where your auth token goes and `path` is how you tell it where to find the URL in whatever JSON your server returns.

## Use

```sh
# generate the default config before first use
spout -g

# pipe anything in, get a URL out
flameshot gui -r | spout

# use a specific profile
flameshot gui -r | spout catbox

# override the file extension
cat video.mp4 | spout -x mp4

# override the filename entirely
cat image.png | spout -n my-screenshot.png
```

URL goes to stdout. URL also goes to your clipboard. That's the whole program.

### Flags

| Flag | Description |
|---|---|
| `-g`, `--gen-config` | Generate the default config file in your config directory. |
| `-G`, `--gen-config-force` | Force overwrite your existing config with the default one. |
| `-p`, `--parse` | Parse and validate the config file, then exit. |
| `-n`, `--name NAME` | Override the uploaded filename entirely. |
| `-x`, `--ext EXT` | Override the file extension, ignoring the profile's default. |
| `-v`, `--version` | Print the version and exit. |
| `-h`, `--help` | Print usage and exit. |

## Status

Verified on Linux (Spectacle, Flameshot, Grim, Scrot) and FreeBSD. HTTP/1.1 is strictly enforced for compatibility with legacy backends.

Windows is out of scope — use [ShareX](https://getsharex.com/). macOS is untested due to lack of hardware; it may work with a custom clipboard script, but is unsupported.

## License

[GPL-3.0](LICENSE)
