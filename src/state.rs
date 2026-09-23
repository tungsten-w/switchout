//! Remembers where each screen lives while it is extended, so it can be put
//! back there after being mirrored or disabled.

use crate::hypr::Monitor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub mode: String,
    pub position: String,
    pub scale: f64,
    pub transform: u8,
}

impl Geometry {
    pub fn of(m: &Monitor) -> Self {
        Self {
            mode: m.mode_string(),
            position: format!("{}x{}", m.x, m.y),
            scale: m.scale,
            transform: m.transform,
        }
    }
}

#[derive(Debug, Default)]
pub struct Layouts {
    map: BTreeMap<String, Geometry>,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("switchout/layouts.json"))
}

impl Layouts {
    /// A missing or broken file just means nothing is remembered yet.
    pub fn load() -> Self {
        let map = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { map }
    }

    pub fn get(&self, m: &Monitor) -> Option<&Geometry> {
        self.map.get(m.key())
    }

    /// Records every extended screen and saves if anything changed.
    /// Saving is best effort: failing to write only loses the memory.
    pub fn record(&mut self, monitors: &[Monitor]) {
        let mut changed = false;
        for m in monitors.iter().filter(|m| m.is_extended()) {
            let g = Geometry::of(m);
            if self.map.get(m.key()) != Some(&g) {
                self.map.insert(m.key().to_string(), g);
                changed = true;
            }
        }
        if !changed {
            return;
        }
        if let Some(p) = path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_vec_pretty(&self.map) {
                let _ = std::fs::write(p, json);
            }
        }
    }
}
