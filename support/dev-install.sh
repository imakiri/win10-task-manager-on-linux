#!/bin/bash
# Copyright 2026 Mission Center Developers
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Pulls a branch, builds it and installs Task Manager (UI, magpie daemon, private GPU libraries,
# hardware database, translations, desktop entry, icons) for the current user, to test changes
# beside or instead of a packaged install.
# Usage: support/dev-install.sh [BRANCH] [--remote URL] [--prefix DIR] [--no-pull] [--uninstall]
#   BRANCH      default: the current branch
#   --remote    repository to pull from (default: the fork's URL below, added as remote "fork")
#   --prefix    default: ~/.local (its bin/ must come before /usr/bin in PATH)
#   --no-pull   build the working tree as it is

set -euo pipefail

# git checkout may replace this file while it runs: bash must read all of it first

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REMOTE_URL="https://github.com/imakiri-twin/win10-task-manager-on-linux.git"
PREFIX="$HOME/.local"
APP_ID="io.github.taskmanager.TaskManager"
branch=""
pull=true
uninstall=false

main() {
    while (($#)); do
        case "$1" in
            --remote) REMOTE_URL="$2"; shift ;;
            --prefix) PREFIX="$2"; shift ;;
            --no-pull) pull=false ;;
            --uninstall) uninstall=true ;;
            -h | --help)
                sed -n '4,12p' "$0" | sed 's/^# \{0,1\}//'
                exit 0
                ;;
            -*) echo "Unknown option: $1" >&2; exit 2 ;;
            *) branch="$1" ;;
        esac
        shift
    done

    if $uninstall; then
        rm -f "$PREFIX/bin/taskmanager" "$PREFIX/bin/missioncenter-magpie"
        rm -rf "$PREFIX/lib/taskmanager" "$PREFIX/share/missioncenter"
        rm -f "$PREFIX/share/applications/$APP_ID.desktop" "$PREFIX/share/metainfo/$APP_ID.metainfo.xml"
        find "$PREFIX/share/icons/hicolor" -name "$APP_ID.png" -delete 2>/dev/null || true
        find "$PREFIX/share/locale" -name taskmanager.mo -delete 2>/dev/null || true
        echo "Removed the build from $PREFIX."
        exit 0
    fi

    cd "$ROOT"
    if $pull; then
        if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
            echo "The working tree has uncommitted changes; commit or stash them, or use --no-pull." >&2
            exit 1
        fi
        remote="$(git remote -v | awk -v url="$REMOTE_URL" '$2 == url { print $1; exit }')"
        if [[ -z "$remote" ]]; then
            remote=fork
            git remote add "$remote" "$REMOTE_URL"
        fi
        branch="${branch:-$(git branch --show-current)}"
        git fetch "$remote" "$branch"
        if git show-ref --verify --quiet "refs/heads/$branch"; then
            git checkout "$branch"
            git merge --ff-only "$remote/$branch"
        else
            git checkout -b "$branch" --track "$remote/$branch"
        fi
        git submodule update --init --recursive
    fi

    echo "Building and installing $(git rev-parse --short HEAD) ($(git branch --show-current)) into $PREFIX…"
    cargo xtask install --prefix "$PREFIX"

    # The running UI and daemon keep the old binaries until restarted
    if pgrep -x taskmanager >/dev/null; then
        echo "Task Manager is running: close it to start the new build."
    fi

    case ":$PATH:" in
        *":$PREFIX/bin:"*) ;;
        *) echo "Note: $PREFIX/bin is not in PATH." >&2 ;;
    esac
    found="$(command -v taskmanager || true)"
    if [[ -n "$found" && "$found" != "$PREFIX/bin/taskmanager" ]]; then
        echo "Note: 'taskmanager' resolves to $found; start $PREFIX/bin/taskmanager directly." >&2
    fi
    echo "Done. Start: $PREFIX/bin/taskmanager   Remove: $0 --uninstall"
}

main "$@"
