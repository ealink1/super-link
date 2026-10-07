#!/bin/sh
# Install the portable package into a user-writable directory for in-app updates.
set -eu
source_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
install_home="${SUPERLINK_INSTALL_HOME:-$HOME}"
target_dir="${XDG_DATA_HOME:-$install_home/.local/share}/superlink/app"
if [ -e "$target_dir" ]; then
    echo "SuperLink is already installed at $target_dir. Use its updater or back up/remove the old application directory first." >&2
    exit 1
fi
if [ -e "$install_home/.local/bin/superlink" ] || [ -L "$install_home/.local/bin/superlink" ]; then
    echo "The SuperLink launcher already exists; installation stopped without overwriting it." >&2
    exit 1
fi
mkdir -p "$(dirname -- "$target_dir")" "$install_home/.local/bin" "${XDG_DATA_HOME:-$install_home/.local/share}/applications"
cp -R "$source_dir" "$target_dir"
ln -s "$target_dir/superlink" "$install_home/.local/bin/superlink"
desktop_file="${XDG_DATA_HOME:-$install_home/.local/share}/applications/superlink.desktop"
# Escape desktop-entry quoted argument characters in user-selected paths.
escaped_dir=$(printf '%s' "$target_dir" | sed 's/[\\"`$]/\\&/g; s/%/%%/g')
cat > "$desktop_file" <<DESKTOP
[Desktop Entry]
Type=Application
Name=SuperLink
Comment=Database, SSH and notes workspace
Exec="$escaped_dir/superlink"
Icon=$target_dir/superlink.png
Terminal=false
Categories=Development;Database;
DESKTOP
printf 'Installed SuperLink in %s\n' "$target_dir"
