#!/bin/sh
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"

echo "Pulling latest changes from git..."
git fetch origin
git pull --rebase origin HEAD || git pull origin HEAD

echo "Building spout in release mode..."
cargo build --release

echo "Installing to $BINDIR..."
mkdir -p "$BINDIR"
cp target/release/spout "$BINDIR/spout"

echo "spout installed successfully! 🎉"

if ! echo "$PATH" | grep -Eq "(^|:)$BINDIR(:|$)"; then
    echo ""
    echo "Warning: $BINDIR is not in your PATH."
    echo "You may need to add it to your ~/.bashrc or ~/.profile:"
    echo "  export PATH=\"\$PATH:$BINDIR\""
fi
