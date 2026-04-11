#!/bin/sh
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
TARGET="$BINDIR/spout"

if [ -f "$TARGET" ]; then
    echo "Removing $TARGET..."
    rm "$TARGET"
    echo "spout uninstalled successfully! 🗑️"
else
    echo "spout is not installed at $TARGET."
fi
