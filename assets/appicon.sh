#!/bin/sh
# Rebuilds the two icon files the bundle needs. Nothing runs this automatically:
# CI has no SVG renderer, iconutil exists only on macOS, and the results are
# committed. Run it when the mark changes.
#
# The bytes it produces are not stable across Pillow and iconutil versions, so a
# run can show a diff even when the mark did not move. What has to hold is the
# layer set and the Dock margin, and `tests/icons.rs` is what holds them.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
mark="$root/assets/brand/coffer-icon-dark-accent.svg"
icons="$root/crates/vault-gui/icons"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
iconset="$work/Coffer.iconset"
mkdir "$iconset"

# iconutil reads each layer's size out of its name and refuses a file whose
# pixels disagree with it, so this table is the specification rather than a
# convenience.
for layer in 16x16:16 16x16@2x:32 32x32:32 32x32@2x:64 128x128:128 \
    128x128@2x:256 256x256:256 256x256@2x:512 512x512:512 512x512@2x:1024; do
    python3 "$root/assets/rasterise.py" "$mark" \
        "$iconset/icon_${layer%:*}.png" "${layer#*:}"
done

iconutil --convert icns --output "$icons/icon.icns" "$iconset"

# The window icon is decoded to raw pixels and compiled into the binary, so a
# 1024 square would spend four megabytes of a fifteen megabyte budget on an
# image macOS never draws.
python3 "$root/assets/rasterise.py" "$mark" "$icons/icon.png" 128
