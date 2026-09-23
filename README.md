# switchout

Quickly switch external screens between **mirror** and **extend** on Hyprland, Windows+P style: a keybind opens a [Quickshell](https://quickshell.org/) menu, you pick a mode, done. A terminal version (TUI) does the same when no shell is running.

```sh
curl -fsSL https://raw.githubusercontent.com/tungsten-w/switchout/main/install.sh | bash
```

Then press **`SUPER + D`**.

## Requirements

- **Arch Linux** (and derivatives, e.g. CachyOS)
- **Hyprland ≥ 0.55 with the Lua config** (`hyprland.lua`). Changes go through `hyprctl eval 'hl.monitor({...})'`.
  The old `hyprctl keyword monitor …` does nothing with the Lua config (it even returns success), and hyprlang configs are removed in 0.57, so they are not supported.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/tungsten-w/switchout/main/install.sh | bash
```

The script installs what is missing (`rust`, `quickshell`) with pacman, builds `switchout` into `~/.cargo/bin`, and runs `switchout setup`, which adds the keybind to `hyprland.lua`:

- it refuses a key that is already taken (pick another one with `--key`),
- it checks the new config with `Hyprland --verify-config` before writing it, and keeps a backup (`hyprland.lua.bak-switchout`),
- the bind lives between `-- >>> switchout` / `-- <<< switchout` markers, and nothing else in the file is touched.

```sh
# other key
curl -fsSL https://raw.githubusercontent.com/tungsten-w/switchout/main/install.sh | bash -s -- --key "SUPER + SHIFT + D"
# remove everything
curl -fsSL https://raw.githubusercontent.com/tungsten-w/switchout/main/install.sh | bash -s -- --uninstall
```

<details>
<summary>Other ways</summary>

**From a clone**

```sh
git clone https://github.com/tungsten-w/switchout && cd switchout && ./install.sh
```

**As a pacman package** (the Quickshell menu is embedded in the binary, so it is a single file)

```sh
git clone https://github.com/tungsten-w/switchout && cd switchout/packaging
makepkg -si
switchout setup        # as your user, adds the keybind
```

**By hand**

```sh
cargo install --locked --git https://github.com/tungsten-w/switchout
switchout setup                    # or add it yourself:
# hl.bind("SUPER + D", hl.dsp.exec_cmd("switchout menu"))
```

</details>

## Modes

| Mode              | What happens                                             |
| ----------------- | -------------------------------------------------------- |
| **Mirror**        | External screens show the same thing as the laptop panel |
| **Extend**        | External screens are extra desktop space                 |
| **External only** | Only the external screens are on (laptop panel off)      |
| **Internal only** | Only the laptop panel is on (external screens off)       |

- The **primary** screen is the laptop panel (`eDP-*`, `LVDS-*`, `DSI-*`), or the focused screen on a desktop.
- By default a mode applies to **every** external screen (HDMI, DisplayPort, USB-C…). With several of them you can target a single one.
- Headless / virtual outputs (`HEADLESS-*`, e.g. for VNC) are ignored unless named explicitly.
- Switching is **runtime only**: your monitor config is never rewritten. `switchout reset` (or `hyprctl reload`) goes back to it.
  The only file switchout edits is `hyprland.lua`, once, to add the keybind (`switchout setup`).
- When a screen is extended again after being mirrored or turned off, it goes back to where it was (remembered per physical screen in `~/.local/state/switchout/layouts.json`).

## Usage

### Quickshell menu

`switchout menu` opens it, and closes it if it is already open (so the keybind toggles it).

| Key                | Action                                   |
| ------------------ | ---------------------------------------- |
| `←` `→` / `h` `l`  | Choose a mode                            |
| `Enter` / click    | Apply and close                          |
| `1`–`4`            | Apply a mode directly                    |
| `Tab`              | Choose the target screen (if several)    |
| `Esc` / click outside | Close                                 |

### TUI

```sh
switchout            # or: switchout tui
```

`↑↓` mode · `Tab` target screen · `←→` where to put the screen when extending · `Enter`/`1-4` apply · `r` back to config · `q` quit.

### CLI

```sh
switchout status                         # screens + current mode
switchout status --json                  # what the Quickshell menu reads
switchout apply mirror
switchout apply extend --side left       # left | right | up | down (default: last position)
switchout apply external-only -o HDMI-A-1
switchout apply internal-only
switchout apply mirror --dry-run         # print the Lua rules instead of applying them
switchout reset                          # hyprctl reload
switchout menu                           # open / close the Quickshell menu
switchout setup [--key "SUPER + D"]      # add the keybind to hyprland.lua
switchout setup --remove                 # remove it
```

## How it works

```
 Quickshell menu (QML) ─┐
                        ├──> switchout (Rust) ──> hyprctl -j monitors all   (read)
 TUI (ratatui) ─────────┘                    └──> hyprctl eval hl.monitor(…) (apply)
```

All the logic lives in the Rust binary; the QML menu only draws and calls `switchout status --json` / `switchout apply …`. The QML is embedded in the binary (`include_str!`) and written to `$XDG_RUNTIME_DIR/switchout/` when the menu opens, so there are no files to install besides the binary.

Things learned the hard way (see also [Panorama's notes](https://github.com/arashonfire/panorama)):

- `hl.monitor` **merges** into the previous rule for the same output, so switchout always sends a **complete** rule (`disabled`, `mode`, `position`, `scale`, `transform`, `mirror`), otherwise a stale `mirror` or `disabled` sticks.
- Rules are sent **one `eval` per screen**: in a single batch, mirroring a screen that the same batch turns back on is silently ignored.
- When turning the laptop panel off, external screens are turned on **first**, so there is never a moment with no screen.
- `mirrorOf` in `hyprctl monitors` is a monitor **id**, not a name.
- Never send a `desc:` rule with an empty description: it matches **every** screen.
- After applying, switchout reads the state back and warns if a screen did not switch (e.g. another rule or tool such as kanshi overrides it).

## Prior art

| Project | Stack | Notes |
| ------- | ----- | ----- |
| [Hypr-Dual-Monitor-Switcher](https://github.com/earthwrld/Hypr-Dual-Monitor-Switcher) | Bash + rofi | Same 4 modes, one external screen, `hyprctl keyword` (broken with the Lua config) |
| [hyprland-display-switcher](https://github.com/FilipJur/hyprland-display-switcher) | Python + GTK3 | Windows+P overlay, `hyprctl keyword` + reload |
| [Panorama](https://github.com/arashonfire/panorama) | Quickshell | Full display settings app, `hyprctl eval`, writes `monitors.lua` |
| [hyprmon](https://github.com/erans/hyprmon) | Go TUI | Layout editor with profiles |
| [HyprDynamicMonitors](https://github.com/fiffeek/hyprdynamicmonitors) | Go daemon + TUI | Automatic profiles on hotplug / power state |
| [nwg-displays](https://github.com/nwg-piotr/nwg-displays), [kanshi](https://sr.ht/~emersion/kanshi/), [shikane](https://gitlab.com/w0lff/shikane) | GTK / C / Rust | Layout editors / profile daemons |

switchout sits in between: not a layout editor, just the quick "I plugged a screen in, mirror or extend?" switch.

## Project layout

```
src/main.rs          CLI (clap) + apply-and-verify
src/hypr.rs          hyprctl: read monitors, eval Lua, reload
src/plan.rs          modes → hl.monitor rules (+ tests)
src/state.rs         remembered layouts (~/.local/state/switchout)
src/tui.rs           terminal menu (ratatui)
src/menu.rs          opens/closes the Quickshell menu
src/setup.rs         adds/removes the keybind in hyprland.lua
quickshell/shell.qml the menu, embedded in the binary at build time
install.sh           one-command installer
packaging/PKGBUILD   pacman package (switchout-git)
```

## Development

```sh
cargo test                                             # plan / Lua / setup tests
cargo run -- apply mirror --dry-run                    # see the rules without applying them
SWITCHOUT_BIN=$PWD/target/debug/switchout qs -p ./quickshell   # iterate on the QML without rebuilding
```

`switchout menu` uses the QML embedded at build time, so run `cargo build` after editing `shell.qml`.

To try modes without touching your real screens, use headless outputs:

```sh
hyprctl output create headless   # twice → e.g. HEADLESS-4, HEADLESS-5
switchout apply mirror --primary HEADLESS-4 -o HEADLESS-5
hyprctl output remove HEADLESS-5   # re-enable / unmirror it first
hyprctl reload                     # drop the test rules
```

## Roadmap

To be defined. Ideas so far:

- Use the Noctalia / system theme colours in the menu
- Open the menu automatically when a screen is plugged in
- Put workspaces back where they were after mirroring (Hyprland moves them)
- Choose left / right when extending, from the Quickshell menu (already in the TUI and CLI)
- Prebuilt binaries in GitHub releases, so installing does not need Rust
- Publish `switchout-git` on the AUR
- Other compositors (Sway, niri…) and distributions
