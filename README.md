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
If you're on Arch, you can install `spout` from the AUR using your favorite helper (like `paru` or `yay`):

```sh
paru -S spout      # stable
paru -S spout-git  # git HEAD
```

## Configure

The default config is in the repo (`config.kdl`) — copy it to `~/.config/spout/config.kdl` and edit to taste. Check the repo for the latest version if something changes.

```kdl
// Default profile when none is specified
default "zendesk"

// Clipboard command — receives the result URL on stdin
// Wayland:
clipboard "wl-copy"
// X11:
// clipboard "xclip" "-selection" "clipboard"
// clipboard "xsel" "--clipboard" "--input"

profile "zendesk" {
    url "https://support.zendesk.com/api/v2/uploads.json?filename={filename}"
    method "POST"
    format "binary"
    header "Content-Type" "application/octet-stream"

    // Dot-separated path into the JSON response to find the URL
    path "upload.attachment.mapped_content_url"

    filename prefix="spout_" random=8 extension="png"
}

profile "catbox" {
    url "https://catbox.moe/user/api.php"
    method "POST"
    format "multipart"
    file-field "fileToUpload"
    field "reqtype" "fileupload"

    // Catbox returns a plain URL string, not JSON — "." means use the raw body
    path "."

    filename random=8 extension="png"
}
```

The config format is [KDL](https://kdl.dev). It's like JSON but for humans.

### Profile options

| Field | Description |
|---|---|
| `url` | Upload endpoint. `{filename}` gets replaced with the generated name. |
| `method` | `POST` or `PUT` |
| `format` | `multipart` or `binary` |
| `file-field` | The multipart field name for the file (defaults to `file`) |
| `field` | Extra multipart fields, repeatable |
| `header` | Extra headers — auth tokens, content types, etc. Repeatable. |
| `path` | Dot-separated path to the URL in the JSON response. Use `"."` for plain-text responses. |
| `filename` | `prefix`, `random` (N random hex bytes), `extension` — all optional |

## Use

```sh
# default profile
flameshot gui -r | spout

# pick a profile
flameshot gui -r | spout catbox

# works with anything
cat image.png | spout
```

URL goes to stdout. URL also goes to your clipboard. That's the whole program.

## Status

Early days. Tested on Linux with Flameshot. Forces HTTP/1.1 because some backends (PHP especially) don't handle HTTP/2 well.

## License

[GPL-3.0](LICENSE)
