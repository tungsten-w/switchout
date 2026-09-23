//! Turns "put the screens in mode X" into a list of `hl.monitor` rules.

use crate::hypr::Monitor;
use crate::state::{Geometry, Layouts};
use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// External screens show the same thing as the primary screen
    Mirror,
    /// External screens are extra desktop space
    Extend,
    /// Only the external screens are on
    #[value(alias = "external")]
    ExternalOnly,
    /// Only the primary screen is on
    #[value(alias = "internal")]
    InternalOnly,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Mirror, Mode::Extend, Mode::ExternalOnly, Mode::InternalOnly];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Mirror => "Mirror",
            Mode::Extend => "Extend",
            Mode::ExternalOnly => "External only",
            Mode::InternalOnly => "Internal only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl Side {
    pub fn position(self) -> &'static str {
        match self {
            Side::Left => "auto-left",
            Side::Right => "auto-right",
            Side::Up => "auto-up",
            Side::Down => "auto-down",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Disable { output: String },
    Enable { output: String, geometry: Geometry, mirror: Option<String> },
}

impl Rule {
    /// Always a complete rule: `hl.monitor` merges into the previous rule for the
    /// same output, so a partial one would keep a stale `mirror` or `disabled`.
    pub fn to_lua(&self) -> String {
        match self {
            Rule::Disable { output } => {
                format!("hl.monitor({{ output = {}, disabled = true }})", lua_str(output))
            }
            Rule::Enable { output, geometry: g, mirror } => format!(
                "hl.monitor({{ output = {}, disabled = false, mode = {}, position = {}, scale = {}, transform = {}, mirror = {} }})",
                lua_str(output),
                lua_str(&g.mode),
                lua_str(&g.position),
                lua_num(g.scale),
                g.transform,
                lua_str(mirror.as_deref().unwrap_or("")),
            ),
        }
    }
}

pub fn script(rules: &[Rule]) -> String {
    rules.iter().map(Rule::to_lua).collect::<Vec<_>>().join("\n")
}

/// Monitor names come from the outside world: quote and escape them.
fn lua_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 32 || c as u32 == 127 => out.push_str(&format!("\\{:03}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn lua_num(x: f64) -> String {
    let rounded = (x * 1e6).round() / 1e6;
    format!("{rounded}")
}

/// The screens a mode is applied to.
#[derive(Debug, Clone)]
pub struct Setup {
    pub primary: Monitor,
    pub targets: Vec<Monitor>,
}

impl Setup {
    /// Primary: `--primary`, else the laptop panel, else the focused screen.
    /// Targets: `--output`s, else every other physical screen.
    pub fn resolve(monitors: &[Monitor], primary: Option<&str>, outputs: &[String]) -> Result<Self> {
        let find = |name: &str| monitors.iter().find(|m| m.name == name);
        let primary = match primary {
            Some(name) => match find(name) {
                Some(m) => m,
                None => bail!("no screen named {name}"),
            },
            None => match monitors
                .iter()
                .find(|m| m.is_internal())
                .or_else(|| monitors.iter().find(|m| m.focused))
                .or_else(|| monitors.first())
            {
                Some(m) => m,
                None => bail!("no screen found"),
            },
        };

        let targets: Vec<Monitor> = if outputs.is_empty() {
            monitors
                .iter()
                .filter(|m| m.name != primary.name && !m.is_virtual())
                .cloned()
                .collect()
        } else {
            let mut targets = Vec::new();
            for name in outputs {
                match find(name) {
                    Some(m) if m.name == primary.name => bail!("{name} is the primary screen"),
                    Some(m) => targets.push(m.clone()),
                    None => bail!("no screen named {name}"),
                }
            }
            targets
        };
        if targets.is_empty() {
            bail!("no external screen connected");
        }
        Ok(Self { primary: primary.clone(), targets })
    }

    /// The mode the screens are in right now, if it is one of ours.
    pub fn current_mode(&self) -> Option<Mode> {
        let p = &self.primary;
        let t = &self.targets;
        if t.iter().all(|m| m.disabled) {
            Some(Mode::InternalOnly)
        } else if p.disabled {
            t.iter().all(|m| m.is_extended()).then_some(Mode::ExternalOnly)
        } else if t.iter().all(|m| !m.disabled && m.mirror_of == p.id.to_string()) {
            Some(Mode::Mirror)
        } else if t.iter().all(|m| m.is_extended()) {
            Some(Mode::Extend)
        } else {
            None
        }
    }

    pub fn plan(&self, mode: Mode, side: Option<Side>, layouts: &Layouts) -> Vec<Rule> {
        let mut rules = Vec::new();
        let enable = |m: &Monitor, position: Option<&str>, mirror: Option<&str>| {
            let mut geometry = restore(m, layouts);
            if let Some(p) = position {
                geometry.position = p.to_string();
            }
            Rule::Enable {
                output: m.name.clone(),
                geometry,
                mirror: mirror.map(str::to_string),
            }
        };

        if mode != Mode::ExternalOnly && !self.primary.is_extended() {
            rules.push(enable(&self.primary, None, None));
        }
        for t in &self.targets {
            match mode {
                Mode::Mirror => rules.push(enable(t, None, Some(&self.primary.name))),
                Mode::Extend | Mode::ExternalOnly => {
                    rules.push(enable(t, side.map(Side::position), None))
                }
                Mode::InternalOnly => rules.push(Rule::Disable { output: t.name.clone() }),
            }
        }
        // Targets are switched on first so there is never a moment with no screen.
        if mode == Mode::ExternalOnly {
            rules.push(Rule::Disable { output: self.primary.name.clone() });
        }
        rules
    }
}

/// Where a screen goes when it is (re)enabled: where it is now if it is already
/// extended, else where it was last time it was, else next to the others.
fn restore(m: &Monitor, layouts: &Layouts) -> Geometry {
    if m.is_extended() {
        return Geometry::of(m);
    }
    if let Some(g) = layouts.get(m) {
        return g.clone();
    }
    Geometry {
        mode: if m.disabled { "preferred".into() } else { m.mode_string() },
        position: "auto-right".into(),
        scale: m.scale,
        transform: m.transform,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: i64, name: &str) -> Monitor {
        Monitor {
            id,
            name: name.into(),
            description: String::new(),
            width: 1920,
            height: 1080,
            refresh_rate: 60.0,
            x: 0,
            y: 0,
            scale: 1.0,
            transform: 0,
            disabled: false,
            mirror_of: "none".into(),
            available_modes: vec!["1920x1080@60.00Hz".into()],
            focused: false,
        }
    }

    fn setup(ms: &[Monitor]) -> Setup {
        Setup::resolve(ms, None, &[]).unwrap()
    }

    #[test]
    fn picks_laptop_as_primary_and_skips_headless() {
        let ms = [mon(0, "DP-1"), mon(1, "eDP-1"), mon(2, "HEADLESS-2")];
        let s = setup(&ms);
        assert_eq!(s.primary.name, "eDP-1");
        assert_eq!(s.targets.iter().map(|m| &m.name[..]).collect::<Vec<_>>(), ["DP-1"]);
    }

    #[test]
    fn errors_without_external_screen() {
        assert!(Setup::resolve(&[mon(0, "eDP-1")], None, &[]).is_err());
    }

    #[test]
    fn mirror_is_a_complete_rule() {
        let s = setup(&[mon(0, "eDP-1"), mon(1, "HDMI-A-1")]);
        let rules = s.plan(Mode::Mirror, None, &Layouts::default());
        assert_eq!(
            script(&rules),
            r#"hl.monitor({ output = "HDMI-A-1", disabled = false, mode = "1920x1080@60.00", position = "0x0", scale = 1, transform = 0, mirror = "eDP-1" })"#
        );
    }

    #[test]
    fn external_only_enables_before_disabling() {
        let mut ext = mon(1, "HDMI-A-1");
        ext.disabled = true;
        let s = setup(&[mon(0, "eDP-1"), ext]);
        let rules = s.plan(Mode::ExternalOnly, None, &Layouts::default());
        assert!(matches!(&rules[0], Rule::Enable { output, .. } if output == "HDMI-A-1"));
        assert_eq!(rules[1], Rule::Disable { output: "eDP-1".into() });
    }

    #[test]
    fn extend_reenables_primary_and_honours_side() {
        let mut laptop = mon(0, "eDP-1");
        laptop.disabled = true;
        let s = setup(&[laptop, mon(1, "HDMI-A-1")]);
        let rules = s.plan(Mode::Extend, Some(Side::Left), &Layouts::default());
        assert_eq!(rules.len(), 2);
        let Rule::Enable { geometry, .. } = &rules[1] else { panic!() };
        assert_eq!(geometry.position, "auto-left");
    }

    #[test]
    fn detects_current_mode() {
        let mut ext = mon(1, "HDMI-A-1");
        assert_eq!(setup(&[mon(0, "eDP-1"), ext.clone()]).current_mode(), Some(Mode::Extend));
        ext.mirror_of = "0".into();
        assert_eq!(setup(&[mon(0, "eDP-1"), ext.clone()]).current_mode(), Some(Mode::Mirror));
        ext.disabled = true;
        assert_eq!(setup(&[mon(0, "eDP-1"), ext]).current_mode(), Some(Mode::InternalOnly));
    }

    #[test]
    fn escapes_lua_strings() {
        assert_eq!(lua_str("a\"b\\c\n"), r#""a\"b\\c\n""#);
        assert_eq!(lua_num(1.3333334), "1.333333");
    }
}
