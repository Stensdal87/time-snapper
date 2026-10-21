#!/bin/sh
# Installerer Time Snapper for den aktuelle bruger (kræver ikke root):
#   ~/.local/bin/time-snapper                  programmet
#   ~/.local/share/applications/…desktop       genvej i programmenuen
#   ~/.local/share/icons/hicolor/…             ikon
# Kør igen for at opdatere. Dine data i ~/.local/share/timesnapper røres ikke.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
exe="$bin_dir/time-snapper"

# Stier i Exec= skal citeres og have \ " ` $ escapet (Desktop Entry-specifikationen).
exec_line=$(printf '%s' "$exe" | sed 's/[\\"`$]/\\&/g')

install -Dm755 "$here/time-snapper" "$exe"
install -Dm644 "$here/time-snapper.png" "$data_dir/icons/hicolor/256x256/apps/timesnapper.png"
mkdir -p "$data_dir/applications"
cat > "$data_dir/applications/timesnapper.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Time Snapper
Comment=Tidsregistrering for kunder og projekter
Exec="$exec_line"
Icon=timesnapper
Terminal=false
Categories=Office;
StartupWMClass=timesnapper
EOF

# Er automatisk start slået til (fra en tidligere version eller et andet sted),
# så peg den på den installerede version.
autostart="$config_dir/autostart/time-snapper.desktop"
if [ -f "$autostart" ]; then
    sed -i "s|^Exec=.*|Exec=\"$(printf '%s' "$exec_line" | sed 's/[|&\\]/\\&/g')\"|" "$autostart"
fi

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$data_dir/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$data_dir/icons/hicolor" 2>/dev/null || true

echo "Time Snapper er installeret i $exe"
echo "Start det fra programmenuen, eller kør: time-snapper"
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "Bemærk: $bin_dir er ikke i din PATH, så kommandoen virker først efter næste login." ;;
esac
