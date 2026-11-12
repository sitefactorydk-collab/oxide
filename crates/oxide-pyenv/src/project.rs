//! The files embedded into the app: the packages' project and lock, and the launcher.

use oxide_core::PYTHON_VERSION;

/// `python/pyproject.toml`.
pub(crate) const PYPROJECT: &str = include_str!(concat!(env!("OUT_DIR"), "/pyproject.toml"));
/// `python/uv.lock` (empty when the repository has none yet).
pub(crate) const LOCK: &str = include_str!(concat!(env!("OUT_DIR"), "/uv.lock"));
/// The launcher `run_python` starts scripts through.
pub(crate) const RUNNER: &str = include_str!("oxide_run.py");

// `AGENT_FILES`: the browser agent's files (`python/oxide_browser_agent/*.py`), name and text.
include!(concat!(env!("OUT_DIR"), "/agent_files.rs"));

/// A stable hash (FNV-1a) of what defines the environment. A change means the packages must
/// be installed again.
pub(crate) fn project_hash() -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in [PYPROJECT, LOCK, PYTHON_VERSION] {
        for b in part.bytes().chain(std::iter::once(0)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_is_embedded_and_hashes_stably() {
        assert!(PYPROJECT.contains("pandas"), "{PYPROJECT}");
        assert!(RUNNER.contains("run_path"));
        assert_eq!(project_hash(), project_hash());
        assert_eq!(project_hash().len(), 16);
        let names: Vec<&str> = AGENT_FILES.iter().map(|(n, _)| *n).collect();
        for want in [
            "__main__.py",
            "server.py",
            "host.py",
            "agent.py",
            "pagetext.py",
        ] {
            assert!(names.contains(&want), "{names:?}");
        }
        assert!(PYPROJECT.contains("[dependency-groups]"));
    }
}
