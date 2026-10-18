/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Control group of each process for the Processes tab's app groups: the cgroup v2 path from
//! `/proc/<pid>/cgroup`. Runs on the gatherer thread only. Read every cycle (no cache): desktops
//! move a freshly started app into its own scope right after launch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use magpie_types::processes::Process;

/// The unified (v2) hierarchy path, e.g. `/user.slice/…/app.slice/app-x-1.scope`.
pub fn parse_cgroup(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .filter(|path| path.starts_with('/'))
}

pub struct CgroupReader {
    root: PathBuf,
}

impl Default for CgroupReader {
    fn default() -> Self {
        Self::with_root("/")
    }
}

impl CgroupReader {
    /// `root` replaces `/` (tests).
    pub fn with_root(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_owned(),
        }
    }

    /// `pid → cgroup path` for `processes`; unreadable or v1-only entries are left out.
    pub fn read(&self, processes: &HashMap<u32, Process>) -> HashMap<u32, String> {
        let mut out = HashMap::with_capacity(processes.len());
        for &pid in processes.keys() {
            let file = self.root.join(format!("proc/{pid}/cgroup"));
            let text = std::fs::read_to_string(file).unwrap_or_default();
            if let Some(path) = parse_cgroup(&text) {
                out.insert(pid, path.to_owned());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_path_is_parsed() {
        assert_eq!(
            parse_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/a.scope\n"),
            Some("/user.slice/user-1000.slice/user@1000.service/app.slice/a.scope")
        );
        // Hybrid systems list v1 controllers before the unified line
        assert_eq!(
            parse_cgroup("12:cpu,cpuacct:/user.slice\n0::/init.scope\n"),
            Some("/init.scope")
        );
        assert_eq!(parse_cgroup("1:name=systemd:/user.slice\n"), None);
    }

    #[test]
    fn reads_from_a_fake_root() {
        let root = std::env::temp_dir().join(format!("mc-cgroups-{}", std::process::id()));
        std::fs::create_dir_all(root.join("proc/42")).unwrap();
        std::fs::write(root.join("proc/42/cgroup"), "0::/app.slice/a.scope\n").unwrap();
        let mut processes = HashMap::new();
        processes.insert(42, Process::default());
        processes.insert(43, Process::default());
        let cgroups = CgroupReader::with_root(&root).read(&processes);
        assert_eq!(
            cgroups.get(&42).map(String::as_str),
            Some("/app.slice/a.scope")
        );
        assert!(!cgroups.contains_key(&43), "unreadable cgroup is left out");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
