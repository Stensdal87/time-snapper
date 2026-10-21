#!/bin/sh
# Fjerner Time Snapper, som install.sh installerede det.
# Dine data i ~/.local/share/timesnapper bliver liggende.
set -eu

data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"

rm -f "$HOME/.local/bin/time-snapper" \
    "$data_dir/applications/timesnapper.desktop" \
    "$data_dir/icons/hicolor/256x256/apps/timesnapper.png" \
    "$config_dir/autostart/time-snapper.desktop"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$data_dir/applications" || true

echo "Time Snapper er fjernet. Dine data ligger stadig i $data_dir/timesnapper"
