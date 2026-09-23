mod hypr;
mod menu;
mod plan;
mod setup;
mod state;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use plan::{Mode, Rule, Setup, Side};
use serde::Serialize;
use state::Layouts;
use std::time::Duration;

/// Switch external screens between mirror / extend / only modes on Hyprland.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show the screens and the current mode
    Status {
        /// Machine-readable output (used by the Quickshell menu)
        #[arg(long)]
        json: bool,
    },
    /// Put the screens in a mode
    Apply {
        mode: Mode,
        /// Where to put the external screens when extending (default: where they were last time)
        #[arg(long)]
        side: Option<Side>,
        /// Only act on this external screen (repeatable; default: all of them)
        #[arg(short, long = "output")]
        outputs: Vec<String>,
        /// Screen to mirror / keep on (default: the laptop panel)
        #[arg(long)]
        primary: Option<String>,
        /// Print the Lua rules instead of applying them
        #[arg(long)]
        dry_run: bool,
    },
    /// Forget runtime changes and go back to the Hyprland config (`hyprctl reload`)
    Reset,
    /// Interactive terminal menu (default)
    Tui,
    /// Open the Quickshell menu, or close it if it is open
    Menu,
    /// Add the keybind that opens the menu to hyprland.lua
    Setup {
        /// Key combination
        #[arg(long, default_value = "SUPER + D")]
        key: String,
        /// Remove the keybind instead
        #[arg(long, conflicts_with = "key")]
        remove: bool,
    },
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("switchout: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command.unwrap_or(Cmd::Tui) {
        Cmd::Status { json } => status(json),
        Cmd::Apply { mode, side, outputs, primary, dry_run } => {
            let monitors = hypr::monitors()?;
            let setup = Setup::resolve(&monitors, primary.as_deref(), &outputs)?;
            if dry_run {
                let mut layouts = Layouts::load();
                layouts.record(&monitors);
                println!("{}", plan::script(&setup.plan(mode, side, &layouts)));
                return Ok(());
            }
            for warning in apply(&monitors, &setup, mode, side)? {
                eprintln!("switchout: warning: {warning}");
            }
            Ok(())
        }
        Cmd::Reset => hypr::reload(),
        Cmd::Tui => tui::run(),
        Cmd::Menu => menu::toggle(),
        Cmd::Setup { remove: true, .. } => setup::remove(),
        Cmd::Setup { key, .. } => setup::install(&key),
    }
}

/// Applies `mode` and checks that Hyprland did what was asked.
/// Returns warnings for screens that did not end up in the expected state.
pub fn apply(monitors: &[hypr::Monitor], setup: &Setup, mode: Mode, side: Option<Side>) -> Result<Vec<String>> {
    let mut layouts = Layouts::load();
    layouts.record(monitors);
    let rules = setup.plan(mode, side, &layouts);
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    // One eval per rule: in a single batch, mirroring a screen that the same
    // batch re-enables is silently ignored.
    for rule in &rules {
        hypr::eval(&rule.to_lua())?;
    }

    std::thread::sleep(Duration::from_millis(300));
    let after = hypr::monitors()?;
    let id_of = |name: &str| after.iter().find(|m| m.name == name).map(|m| m.id.to_string());
    let mut warnings = Vec::new();
    for rule in &rules {
        let (name, ok) = match rule {
            Rule::Disable { output } => {
                (output, after.iter().any(|m| &m.name == output && m.disabled))
            }
            Rule::Enable { output, mirror, .. } => {
                let ok = after.iter().any(|m| {
                    &m.name == output
                        && !m.disabled
                        && match mirror {
                            Some(src) => Some(&m.mirror_of) == id_of(src).as_ref(),
                            None => !m.is_mirroring(),
                        }
                });
                (output, ok)
            }
        };
        if !ok {
            warnings.push(format!(
                "{name} did not switch as expected (another monitor rule or tool may override it)"
            ));
        }
    }
    Ok(warnings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenJson<'a> {
    name: &'a str,
    description: &'a str,
    primary: bool,
    enabled: bool,
    mirror_of: Option<&'a str>,
    mode: String,
}

#[derive(Serialize)]
struct StatusJson<'a> {
    mode: Option<Mode>,
    screens: Vec<ScreenJson<'a>>,
    error: Option<String>,
}

fn status(json: bool) -> Result<()> {
    let monitors = hypr::monitors()?;
    let setup = Setup::resolve(&monitors, None, &[]);
    let primary = match &setup {
        Ok(s) => Some(s.primary.name.as_str()),
        Err(_) => monitors.iter().find(|m| m.is_internal()).map(|m| m.name.as_str()),
    };
    let screens: Vec<ScreenJson> = monitors
        .iter()
        .filter(|m| !m.is_virtual())
        .map(|m| ScreenJson {
            name: &m.name,
            description: &m.description,
            primary: Some(m.name.as_str()) == primary,
            enabled: !m.disabled,
            mirror_of: monitors
                .iter()
                .find(|src| m.is_mirroring() && src.id.to_string() == m.mirror_of)
                .map(|src| src.name.as_str()),
            mode: m.mode_string(),
        })
        .collect();

    if json {
        let out = StatusJson {
            mode: setup.as_ref().ok().and_then(Setup::current_mode),
            error: setup.as_ref().err().map(|e| e.to_string()),
            screens,
        };
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    for s in &screens {
        let state = match (s.enabled, s.mirror_of) {
            (false, _) => "off".to_string(),
            (true, Some(src)) => format!("mirrors {src}"),
            (true, None) => "on".to_string(),
        };
        let tag = if s.primary { " (primary)" } else { "" };
        println!("{:<10} {:<16} {:<14} {}{tag}", s.name, s.mode, state, s.description);
    }
    match setup {
        Ok(s) => match s.current_mode() {
            Some(mode) => println!("\nmode: {}", mode.label()),
            None => println!("\nmode: custom"),
        },
        Err(e) => println!("\n{e}"),
    }
    Ok(())
}
