//! Noctalia integration: the menu uses Noctalia's live colours, font and corner radius.
//!
//! Noctalia 5 keeps its palette in memory, so switchout registers a user template in
//! `~/.config/noctalia/switchout.toml` (Noctalia loads every `.toml` there). Noctalia then
//! renders the palette to `~/.local/state/switchout/noctalia-colors.json` whenever it changes.
//! The older Quickshell-based Noctalia (4.x) already writes `~/.config/noctalia/colors.json`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Same keys as Noctalia 4's `colors.json`, so the menu reads both the same way.
const TEMPLATE: &str = r#"{
  "mPrimary": "{{colors.primary.default.hex}}",
  "mOnPrimary": "{{colors.on_primary.default.hex}}",
  "mError": "{{colors.error.default.hex}}",
  "mSurface": "{{colors.surface.default.hex}}",
  "mOnSurface": "{{colors.on_surface.default.hex}}",
  "mSurfaceVariant": "{{colors.surface_container.default.hex}}",
  "mOnSurfaceVariant": "{{colors.on_surface_variant.default.hex}}",
  "mOutline": "{{colors.outline_variant.default.hex}}"
}
"#;

/// What the menu should look like; `None` fields keep the menu's built-in theme.
#[derive(Debug, Default, PartialEq)]
pub struct Theme {
    pub colors: Option<PathBuf>,
    pub font: Option<String>,
    pub radius_scale: Option<f64>,
}

fn dir(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
}

fn noctalia_dir() -> Option<PathBuf> {
    Some(dir("XDG_CONFIG_HOME", ".config")?.join("noctalia"))
}

fn state_dir() -> Option<PathBuf> {
    Some(dir("XDG_STATE_HOME", ".local/state")?.join("switchout"))
}

/// Noctalia 5 is a native binary called `noctalia`; Noctalia 4 runs inside Quickshell.
fn has_noctalia5() -> bool {
    Command::new("noctalia")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn toml_string(path: &Path) -> String {
    format!("\"{}\"", path.display().to_string().replace('\\', "\\\\").replace('"', "\\\""))
}

/// Writes `path` unless it already has `content`. Returns whether it changed.
fn write_if_changed(path: &Path, content: &str) -> bool {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return false;
    }
    path.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok()) && std::fs::write(path, content).is_ok()
}

/// Registers the template with Noctalia 5 and returns where the palette is rendered.
/// On first use, asks Noctalia to render it now (it takes a few seconds; the menu
/// picks the file up as soon as it appears).
pub fn install() -> Option<PathBuf> {
    let noctalia = noctalia_dir()?;
    if !noctalia.is_dir() || !has_noctalia5() {
        return None;
    }
    let state = state_dir()?;
    let input = state.join("noctalia-template.json");
    let output = state.join("noctalia-colors.json");
    let config = format!(
        "# Added by switchout so its menu follows the Noctalia palette.\n\
         # Removed by `switchout setup --remove`.\n\
         [theme.templates.user.switchout]\n\
         input_path = {}\n\
         output_path = {}\n",
        toml_string(&input),
        toml_string(&output),
    );
    let changed = write_if_changed(&input, TEMPLATE) | write_if_changed(&noctalia.join("switchout.toml"), &config);
    if changed || !output.exists() {
        for args in [["msg", "config-reload"], ["msg", "templates-apply"]] {
            let _ = Command::new("noctalia").args(args).stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
    }
    Some(output)
}

/// Unregisters the template. Returns whether there was one.
pub fn remove() -> bool {
    let removed = noctalia_dir().is_some_and(|d| std::fs::remove_file(d.join("switchout.toml")).is_ok());
    if let Some(state) = state_dir() {
        let _ = std::fs::remove_file(state.join("noctalia-template.json"));
        let _ = std::fs::remove_file(state.join("noctalia-colors.json"));
    }
    removed
}

/// `key = value` in the `[section]` table of a TOML document (top-level keys of that table only).
fn toml_value<'a>(toml: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let header = format!("[{section}]");
    let mut inside = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim());
        }
    }
    None
}

fn unquote(value: &str) -> Option<String> {
    let s = value.strip_prefix('"')?.strip_suffix('"')?;
    (!s.is_empty()).then(|| s.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn theme5() -> Option<Theme> {
    let colors = install()?;
    let out = Command::new("noctalia").args(["config", "export"]).stderr(Stdio::null()).output().ok();
    let config = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    Some(Theme {
        colors: Some(colors),
        font: toml_value(&config, "shell", "font_family").and_then(unquote),
        radius_scale: toml_value(&config, "shell", "corner_radius_scale").and_then(|v| v.parse().ok()),
    })
}

fn theme4() -> Option<Theme> {
    let dir = noctalia_dir()?;
    let colors = dir.join("colors.json");
    if !colors.exists() {
        return None;
    }
    let settings: serde_json::Value = std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    Some(Theme {
        colors: Some(colors),
        font: settings["ui"]["fontDefault"].as_str().filter(|s| !s.is_empty()).map(String::from),
        radius_scale: settings["general"]["radiusRatio"].as_f64(),
    })
}

pub fn theme() -> Theme {
    theme5().or_else(theme4).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shell_settings() {
        let config = "[bar]\nfont_family = \"Bar\"\n\n[shell]\ncorner_radius_scale = 1.85\n    [shell.x]\nfont_family = \"Nested\"\n[shell]\nfont_family = \"Comfortaa SemiBold\"\n";
        assert_eq!(toml_value(config, "shell", "corner_radius_scale"), Some("1.85"));
        assert_eq!(toml_value(config, "shell", "font_family").and_then(unquote).as_deref(), Some("Comfortaa SemiBold"));
        assert_eq!(toml_value(config, "bar", "radius"), None);
        assert_eq!(unquote("\"\""), None);
    }
}
