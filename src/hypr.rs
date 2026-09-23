//! Talks to Hyprland through `hyprctl`.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f64,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: u8,
    pub disabled: bool,
    /// Id of the monitor being mirrored, or `"none"`.
    pub mirror_of: String,
    #[serde(default)]
    pub available_modes: Vec<String>,
    #[serde(default)]
    pub focused: bool,
}

impl Monitor {
    pub fn is_internal(&self) -> bool {
        ["eDP", "LVDS", "DSI"].iter().any(|p| self.name.starts_with(p))
    }

    /// Headless / nested outputs are never picked as "external screens" unless named explicitly.
    pub fn is_virtual(&self) -> bool {
        ["HEADLESS", "WAYLAND", "X11", "FALLBACK"].iter().any(|p| self.name.starts_with(p))
    }

    pub fn is_mirroring(&self) -> bool {
        self.mirror_of != "none" && !self.mirror_of.is_empty()
    }

    /// Enabled and showing its own content.
    pub fn is_extended(&self) -> bool {
        !self.disabled && !self.is_mirroring()
    }

    /// Current mode as a Hyprland mode string. Prefers an exact entry from
    /// `availableModes` (e.g. `2560x1440@180.00`) over the raw refresh rate.
    pub fn mode_string(&self) -> String {
        let prefix = format!("{}x{}@", self.width, self.height);
        self.available_modes
            .iter()
            .filter_map(|m| {
                let hz: f64 = m.strip_prefix(&prefix)?.trim_end_matches("Hz").parse().ok()?;
                Some((hz, m.trim_end_matches("Hz")))
            })
            .min_by(|a, b| {
                let da = (a.0 - self.refresh_rate).abs();
                let db = (b.0 - self.refresh_rate).abs();
                da.total_cmp(&db)
            })
            .map(|(_, m)| m.to_string())
            .unwrap_or_else(|| format!("{prefix}{:.2}", self.refresh_rate))
    }

    /// Key used to remember a screen's layout: the EDID description when there is one,
    /// so the same physical screen is recognised on any port.
    pub fn key(&self) -> &str {
        if self.description.is_empty() { &self.name } else { &self.description }
    }
}

pub fn monitors() -> Result<Vec<Monitor>> {
    let out = Command::new("hyprctl")
        .args(["-j", "monitors", "all"])
        .output()
        .context("could not run hyprctl (is Hyprland running?)")?;
    if !out.status.success() {
        bail!("hyprctl monitors failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    serde_json::from_slice(&out.stdout).context("could not parse `hyprctl -j monitors all`")
}

/// Runs a Lua snippet with `hyprctl eval` (Hyprland >= 0.55, Lua config).
/// `hyprctl keyword monitor` is a silent no-op with the Lua config, so it is not used.
pub fn eval(script: &str) -> Result<()> {
    let out = Command::new("hyprctl")
        .args(["eval", script])
        .output()
        .context("could not run hyprctl")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || stdout.trim() != "ok" {
        bail!(
            "hyprctl eval failed: {}{}",
            stdout.trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Drops every runtime rule and goes back to the config files.
pub fn reload() -> Result<()> {
    let status = Command::new("hyprctl").arg("reload").status().context("could not run hyprctl")?;
    if !status.success() {
        bail!("hyprctl reload failed");
    }
    Ok(())
}
