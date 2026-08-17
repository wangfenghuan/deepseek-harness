//! Locate the Node/npx executables for a Finder-launched GUI app.
//!
//! macOS apps launched from Finder inherit only `/usr/bin:/bin:/usr/sbin:/sbin`,
//! so nvm, Homebrew, fnm, volta, and mise installs are invisible to `PATH`.
//! This module rebuilds a candidate `PATH` by probing the common installation
//! roots on top of whatever the process already has.

use std::env;
use std::path::{Path, PathBuf};

/// Additional `PATH` directories probed regardless of the inherited value.
const WELL_KNOWN_DIRS: &[&str] = &[
    "/opt/homebrew/bin", // Homebrew (Apple Silicon)
    "/usr/local/bin",    // Homebrew (Intel) and the official Node installer
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
    "/opt/homebrew/opt/node/bin",
    "/usr/local/opt/node/bin",
];

/// Home-relative version-manager bin dirs that may hold `node`/`npx`.
const HOME_BIN_DIRS: &[&str] = &[
    ".volta/bin",
    ".fnm",
    ".local/bin",
    ".bun/bin",
    ".asdf/shims",
    ".local/share/mise/shims",
];

/// Build a `PATH`-style list that includes every plausible Node location.
/// Deduplicated, preserving the inherited `PATH` order first.
pub fn candidate_path() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf| {
        if !dirs.iter().any(|existing| existing == &dir) {
            dirs.push(dir);
        }
    };
    if let Ok(path) = env::var("PATH") {
        for part in path.split(':') {
            if !part.is_empty() {
                push(PathBuf::from(part));
            }
        }
    }
    for dir in WELL_KNOWN_DIRS {
        push(PathBuf::from(dir));
    }
    if let Ok(home) = env::var("HOME") {
        let home = Path::new(&home);
        // nvm keeps one bin dir per installed version; newest version first.
        let nvm_root = home.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_root) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.join("bin").is_dir())
                .collect();
            versions.sort();
            for version in versions.into_iter().rev() {
                push(version.join("bin"));
            }
        }
        for relative in HOME_BIN_DIRS {
            let dir = home.join(relative);
            if dir.is_dir() {
                push(dir);
            }
        }
    }
    dirs
}
