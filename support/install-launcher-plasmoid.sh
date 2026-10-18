#!/bin/bash
# Copyright 2026 Mission Center Developers
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Installs the fullscreen launcher for the current user on KDE Plasma 6:
#   - taskmanager-launcher (the installed one, or built from this checkout into ~/.local/bin),
#   - its D-Bus activation file (Meta/clicks start it on demand),
#   - the "Task Manager Launcher" panel widget.
# Usage: support/install-launcher-plasmoid.sh [--build] [--uninstall]
#   --build      build and install from this checkout even if taskmanager-launcher is installed

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLASMOID="$ROOT/data/plasmoid/io.github.taskmanager.launcher"
PLASMOID_ID="io.github.taskmanager.launcher"
LOCAL_BIN="${HOME}/.local/bin/taskmanager-launcher"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
ICON="io.github.taskmanager.TaskManager.png"

build=false
uninstall=false
for arg in "$@"; do
    case "$arg" in
        --build) build=true ;;
        --uninstall) uninstall=true ;;
        -h | --help)
            sed -n '4,10p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            exit 2
            ;;
    esac
done

command -v kpackagetool6 >/dev/null || {
    echo "kpackagetool6 not found: KDE Plasma 6 is required." >&2
    exit 1
}

launcher() {
    if [[ -x "$LOCAL_BIN" ]]; then
        echo "$LOCAL_BIN"
    else
        command -v taskmanager-launcher || true
    fi
}

if $uninstall; then
    kpackagetool6 --type Plasma/Applet --remove "$PLASMOID_ID" >/dev/null 2>&1 || true
    bin="$(launcher)"
    if [[ -n "$bin" ]]; then
        "$bin" --uninstall-activation
    fi
    # Stop a resident launcher; nothing would start or toggle it anymore
    pkill -x taskmanager-lau 2>/dev/null || true
    rm -f "$LOCAL_BIN"
    echo "Removed. Choose another launcher in the panel (right click → Show Alternatives)."
    exit 0
fi

bin="$(launcher)"
if $build || [[ -z "$bin" ]]; then
    command -v cargo >/dev/null || {
        echo "taskmanager-launcher is not installed and cargo is unavailable to build it." >&2
        exit 1
    }
    echo "Building taskmanager-launcher…"
    cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" -p taskmanager-launcher
    install -Dm755 "$ROOT/target/release/taskmanager-launcher" "$LOCAL_BIN"
    bin="$LOCAL_BIN"
fi

"$bin" --install-activation

# The widget uses the application icon; install it when Task Manager itself is not installed
for size in 16 22 24 32 48 64 128 256; do
    target="$ICONS/${size}x${size}/apps/$ICON"
    if [[ ! -e "/usr/share/icons/hicolor/${size}x${size}/apps/$ICON" && ! -e "$target" ]]; then
        install -Dm644 "$ROOT/data/icons/hicolor/${size}x${size}/apps/$ICON" "$target"
    fi
done

if kpackagetool6 --type Plasma/Applet --show "$PLASMOID_ID" >/dev/null 2>&1; then
    kpackagetool6 --type Plasma/Applet --upgrade "$PLASMOID"
else
    kpackagetool6 --type Plasma/Applet --install "$PLASMOID"
fi

command -v missioncenter-magpie >/dev/null ||
    echo "Note: install Task Manager (it provides the missioncenter-magpie daemon) for live data." >&2

cat <<EOF

Installed: $bin
Next: right click the application menu button in the panel → Show Alternatives →
"Task Manager Launcher". Meta and a click then show or hide the launcher.
If the widget is not listed yet: systemctl --user restart plasma-plasmashell
EOF
