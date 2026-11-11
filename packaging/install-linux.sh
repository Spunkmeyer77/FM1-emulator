#!/bin/sh
# Installs FM-1 emulator for the current user (~/.clap and ~/.vst3).
set -e
cd "$(dirname "$0")"
mkdir -p "$HOME/.clap" "$HOME/.vst3"
cp FM-1.clap "$HOME/.clap/"
rm -rf "$HOME/.vst3/FM-1.vst3"
cp -R FM-1.vst3 "$HOME/.vst3/"
echo "FM-1 emulator installed in ~/.clap and ~/.vst3. Restart your DAW."
