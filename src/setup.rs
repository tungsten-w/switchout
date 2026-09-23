//! Adds (or removes) the keybind that opens the menu, in `hyprland.lua`.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const BEGIN: &str = "-- >>> switchout (managed by `switchout setup`, remove with `switchout setup --remove`)";
const END: &str = "-- <<< switchout";

fn config_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("HOME is not set")?;
    let lua = base.join("hypr/hyprland.lua");
    if !lua.exists() {
        if base.join("hypr/hyprland.conf").exists() {
            bail!("switchout needs the Lua config (hyprland.lua, Hyprland >= 0.55); hyprland.conf is not supported");
        }
        bail!("{} not found", lua.display());
    }
    Ok(lua)
}

/// `"super+shift + d"` → `"SUPER + SHIFT + d"`. Only modifiers are upper-cased:
/// key names like `Return` or `XF86AudioMute` are kept as written.
fn normalize(key: &str) -> Result<String> {
    let mut parts: Vec<String> = key.split('+').map(|p| p.trim().to_string()).collect();
    let n = parts.len();
    for p in &mut parts[..n - 1] {
        *p = p.to_uppercase();
    }
    if parts.iter().any(String::is_empty) {
        bail!("invalid key {key:?} (expected something like \"SUPER + D\")");
    }
    Ok(parts.join(" + "))
}

#[derive(Deserialize)]
struct Bind {
    modmask: u32,
    key: String,
    #[serde(default)]
    arg: String,
    #[serde(default)]
    dispatcher: String,
    #[serde(default)]
    description: String,
}

/// What the key is already bound to, if anything.
/// Best effort: if Hyprland isn't running, there is nothing to check against.
fn conflict(key: &str) -> Option<String> {
    let mut parts: Vec<&str> = key.split(" + ").collect();
    let name = parts.pop()?;
    let mut mask = 0;
    for m in parts {
        mask |= match m {
            "SHIFT" => 1,
            "CAPS" => 2,
            "CTRL" | "CONTROL" => 4,
            "ALT" => 8,
            "MOD2" => 16,
            "MOD3" => 32,
            "SUPER" | "WIN" | "LOGO" | "MOD4" => 64,
            "MOD5" => 128,
            _ => return None,
        };
    }
    let out = Command::new("hyprctl").args(["-j", "binds"]).output().ok()?;
    let binds: Vec<Bind> = serde_json::from_slice(&out.stdout).ok()?;
    let b = binds.into_iter().find(|b| b.modmask == mask && b.key.eq_ignore_ascii_case(name))?;
    // Lua binds only expose a function id, so there is often nothing better to show.
    Some(match (b.description.is_empty(), b.dispatcher.as_str()) {
        (false, _) => b.description,
        (true, "__lua") | (true, "") => "another action".into(),
        (true, d) => format!("{d} {}", b.arg).trim().to_string(),
    })
}

/// The key of the bind currently in our block, if any.
fn current_key(config: &str) -> Option<&str> {
    let mut inside = false;
    for line in config.lines() {
        if line.starts_with("-- >>> switchout") {
            inside = true;
        } else if inside {
            return line.strip_prefix("hl.bind(\"")?.split('"').next();
        }
    }
    None
}

/// The config with any previous switchout block removed.
fn strip_block(config: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in config.lines() {
        if line.starts_with("-- >>> switchout") {
            inside = true;
        } else if inside && line.starts_with(END) {
            inside = false;
        } else if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Checks a candidate config with `Hyprland --verify-config` before it replaces the real one.
fn verify(path: &Path, content: &str) -> Result<()> {
    // Same directory, so `require(...)` resolves like it does for the real file.
    let tmp = path.with_file_name(".hyprland.switchout-check.lua");
    std::fs::write(&tmp, content).with_context(|| format!("could not write {}", tmp.display()))?;
    let out = Command::new("Hyprland").arg("--verify-config").arg("-c").arg(&tmp).output();
    let _ = std::fs::remove_file(&tmp);
    let Ok(out) = out else { return Ok(()) }; // no Hyprland binary to check with
    let text = String::from_utf8_lossy(&out.stdout);
    if !text.contains("config ok") {
        let details = text.rsplit("Config parsing result:").next().unwrap_or(&text).trim();
        let details = details.replace(".hyprland.switchout-check.lua", "hyprland.lua");
        bail!("the new config would not load, nothing was changed:\n{details}");
    }
    Ok(())
}

fn write(path: &Path, content: &str) -> Result<()> {
    verify(path, content)?;
    let backup = path.with_extension("lua.bak-switchout");
    std::fs::copy(path, &backup).with_context(|| format!("could not back up to {}", backup.display()))?;
    std::fs::write(path, content).with_context(|| format!("could not write {}", path.display()))?;
    // `config-only`: pick up the bind without touching the screens.
    let _ = Command::new("hyprctl").args(["reload", "config-only"]).output();
    Ok(())
}

pub fn install(key: &str) -> Result<()> {
    let key = normalize(key)?;
    let path = config_path()?;
    let config = std::fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    if current_key(&config) != Some(key.as_str())
        && let Some(other) = conflict(&key)
    {
        bail!("{key} is already bound to {other}; pick another key, e.g. --key \"SUPER + SHIFT + D\"");
    }

    // Absolute path: Hyprland's PATH may not include ~/.cargo/bin.
    let exe = std::env::current_exe().context("could not find the switchout binary")?;
    let command = format!("{} menu", exe.display()).replace('\\', "\\\\").replace('"', "\\\"");
    let mut new = strip_block(&config);
    if !new.ends_with("\n\n") {
        new.push('\n');
    }
    new.push_str(&format!("{BEGIN}\nhl.bind(\"{key}\", hl.dsp.exec_cmd(\"{command}\"))\n{END}\n"));

    if new == config {
        println!("Already set up: {key} opens the menu.");
        return Ok(());
    }
    write(&path, &new)?;
    println!("Done: press {key} to open the menu.");
    println!("(bind added to {}, previous version saved as hyprland.lua.bak-switchout)", path.display());
    Ok(())
}

pub fn remove() -> Result<()> {
    let path = config_path()?;
    let config = std::fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    let new = strip_block(&config);
    if new == config {
        println!("No switchout keybind in {}.", path.display());
        return Ok(());
    }
    // Also drop the blank line `install` put before the block.
    write(&path, &format!("{}\n", new.trim_end_matches('\n')))?;
    println!("Keybind removed from {}.", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_keys() {
        assert_eq!(normalize("super+d").unwrap(), "SUPER + d");
        assert_eq!(normalize("SUPER + Return").unwrap(), "SUPER + Return");
        assert_eq!(normalize(" SUPER + shift+ F12 ").unwrap(), "SUPER + SHIFT + F12");
        assert!(normalize("SUPER +").is_err());
    }

    #[test]
    fn strips_only_our_block() {
        let config = format!("a\n{BEGIN}\nhl.bind(\"SUPER + D\", x)\n{END}\nb\n");
        assert_eq!(strip_block(&config), "a\nb\n");
        assert_eq!(strip_block("a\nb\n"), "a\nb\n");
        assert_eq!(current_key(&config), Some("SUPER + D"));
        assert_eq!(current_key("a\n"), None);
    }
}
