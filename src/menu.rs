//! Opens the Quickshell menu. The QML is embedded in the binary, so installing
//! `switchout` is all it takes.

use anyhow::{Context, Result, bail};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const QML: &str = include_str!("../quickshell/shell.qml");

/// First `qs` / `quickshell` found in `PATH`.
fn quickshell() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| ["qs", "quickshell"].map(|name| dir.join(name)))
        .find(|p| p.is_file())
}

/// Opens the menu, or closes it if it is already open, so one keybind toggles it.
pub fn toggle() -> Result<()> {
    let Some(qs) = quickshell() else {
        bail!("Quickshell is not installed (sudo pacman -S quickshell)");
    };

    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("switchout");
    let file = dir.join("shell.qml");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(QML) {
        std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
        std::fs::write(&file, QML).with_context(|| format!("could not write {}", file.display()))?;
    }

    let closed = Command::new(&qs)
        .arg("kill")
        .arg("-p")
        .arg(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if closed {
        return Ok(());
    }

    let exe = std::env::current_exe().context("could not find the switchout binary")?;
    let err = Command::new(&qs).arg("-p").arg(&dir).env("SWITCHOUT_BIN", exe).exec();
    Err(err).with_context(|| format!("could not run {}", qs.display()))
}
