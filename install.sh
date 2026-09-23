#!/usr/bin/env bash
# switchout installer (Arch Linux + Hyprland).
#
#   curl -fsSL https://raw.githubusercontent.com/tungsten-w/switchout/main/install.sh | bash
#   ./install.sh                          # from a clone
#   ./install.sh --key "SUPER + SHIFT + D"
#   ./install.sh --uninstall
set -euo pipefail

repo=${SWITCHOUT_REPO:-https://github.com/tungsten-w/switchout}
bin="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/switchout"

say() { printf '\e[1;35m::\e[0m %s\n' "$*"; }
die() { printf '\e[1;31merror:\e[0m %s\n' "$*" >&2; exit 1; }

if [[ ${1:-} == --uninstall ]]; then
    [[ -x $bin ]] && "$bin" setup --remove || true
    cargo uninstall switchout 2>/dev/null || true
    rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/switchout"
    say "switchout removed"
    exit 0
fi

command -v pacman >/dev/null || die "only Arch Linux (and derivatives) is supported for now"
command -v Hyprland >/dev/null || die "Hyprland is not installed"
[[ -f ${XDG_CONFIG_HOME:-$HOME/.config}/hypr/hyprland.lua ]] \
    || die "switchout needs Hyprland's Lua config (~/.config/hypr/hyprland.lua, Hyprland >= 0.55)"

missing=()
command -v cargo >/dev/null || missing+=(rust)
command -v git >/dev/null || missing+=(git)
command -v qs >/dev/null || command -v quickshell >/dev/null || missing+=(quickshell)
if ((${#missing[@]})); then
    say "Installing ${missing[*]}"
    # </dev/tty: when piped from curl, stdin is this script, not the keyboard.
    sudo pacman -S --needed "${missing[@]}" </dev/tty
fi

# Run from a clone → build it; piped from curl → build from GitHub.
src=$(cd "$(dirname "${BASH_SOURCE[0]:-.}")" 2>/dev/null && pwd || true)
if [[ -f $src/Cargo.toml ]] && grep -q '^name = "switchout"' "$src/Cargo.toml"; then
    say "Building switchout from $src"
    cargo install --locked --quiet --path "$src"
else
    say "Building switchout from $repo"
    cargo install --locked --quiet --git "$repo"
fi

say "Adding the keybind"
"$bin" setup "$@" || die "switchout is installed, but the keybind was not added. Retry with: $bin setup --key \"SUPER + SHIFT + D\""
say "All set. Run \`switchout\` in a terminal for the TUI."
