# spout

The missing link between your screenshot tool and the internet.

![Showcase](assets/showcase.gif)

Most screenshot tools want to own your entire workflow: hotkey, capture, upload, all bundled together. spout doesn't care about any of that. It reads bytes from stdin and gives you a URL. What produces those bytes is your problem. It's a pipe segment. That's it.

## Install

```sh
git clone https://github.com/Gur0v/spout
cd spout
./install.sh
```

To uninstall:

```sh
./uninstall.sh
```

**Arch Linux (AUR)**

```sh
paru -S spout      # stable
paru -S spout-git  # git HEAD
```

## Configure

Run `spout -g` to generate the default config at `~/.config/spout/config.kdl`.

`x0` is the default profile. `zendesk`, `ez`, and `imgur` are included as examples for custom backends.

```kdl
default "x0"

clipboard "wl-copy"
// clipboard "xclip" "-selection" "clipboard"
// clipboard "xsel" "--clipboard" "--input"

profile "x0" {
    url "https://x0.at"
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
```

By default, spout enforces safety limits to prevent SSRF, DNS rebinding, and accidental massive uploads (100MB cap). Clipboard interaction is restricted to standard binaries (`wl-copy`, `xclip`, `xsel`). PNG, JPEG, and WebP uploads have embedded metadata stripped before upload. Metadata stripping buffers stdin before upload, including in `yolo` mode.

Set `strip-meta false` inside a profile to keep metadata for that backend.

### Profile options

| Field | Description |
|---|---|
| `url` | Upload endpoint. `{filename}` is replaced with the generated filename. |
| `method` | `POST` or `PUT` |
| `format` | `multipart` or `binary` |
| `file-field` | Multipart field name for the file (defaults to `file`) |
| `field` | Extra multipart fields, repeatable |
| `header` | Extra headers for auth tokens, content types, etc. Repeatable. |
| `path` | Dot-separated path to the URL in the JSON response. Use `"."` for plain-text responses. |
| `strip-meta` | Set to `false` to skip PNG/JPEG/WebP metadata stripping for that profile |
| `filename` | `prefix`, `random` (N random hex chars), `extension` - all optional |

### Expert mode

Adding `yolo true` to the root of your config disables SSRF protection, certificate validation, redirect limits, upload size caps, filename validation, and the clipboard binary allowlist. Intended for non-standard environments where you control the entire stack and know exactly what you are turning off. Don't use it otherwise.

## Use

```sh
# generate the default config before first use
spout -g

# pipe anything in, get a URL out
flameshot gui -r | spout

# use a specific profile
flameshot gui -r | spout zendesk

# override the file extension
spout -x mp4 < video.mp4

# override the filename entirely
spout -n my-screenshot.png < image.png

# print URL but skip clipboard copy
spout -N < image.png
```

URL goes to stdout. URL also goes to your clipboard unless you pass `-N`. That's the whole program.

### Flags

| Flag | Description |
|---|---|
| `-g`, `--gen-config` | Generate the default config file. |
| `-G`, `--gen-config-force` | Overwrite existing config with the default. |
| `-p`, `--parse` | Parse and validate the config file, then exit. |
| `-n`, `--name NAME` | Override the uploaded filename. |
| `-N`, `--no-clipboard` | Skip copying the URL to the clipboard. |
| `-x`, `--ext EXT` | Override the file extension. |
| `-v`, `--version` | Print the version and exit. |
| `-h`, `--help` | Print usage and exit. |

## Status

Verified on Linux (Spectacle, Flameshot, Grim, Scrot), FreeBSD, and OpenBSD. HTTP/1.1 is strictly enforced for compatibility with legacy backends.

Windows is out of scope: use [ShareX](https://getsharex.com/). macOS is currently unsupported.

## License

[GPL-3.0](LICENSE)
