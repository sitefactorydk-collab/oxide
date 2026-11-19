//! App command permissions (Phase 17's capability audit).
//!
//! Every window gets only the commands it uses: `build.rs` declares Oxide's commands to
//! Tauri (read from `collect_commands!` in `lib.rs`), which turns on the access check for
//! them, and `capabilities/*.json` grant `allow-<command>` per window. This file is shared
//! by `build.rs` and the tests below, which keep the capabilities in step with the commands.

/// The command names registered in `collect_commands![…]` of `lib.rs`'s source.
pub fn commands_in(lib_rs: &str) -> Vec<String> {
    let Some(start) = lib_rs.find("collect_commands![") else {
        return Vec::new();
    };
    let body = &lib_rs[start + "collect_commands![".len()..];
    let body = &body[..body.find(']').unwrap_or(body.len())];
    body.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .flat_map(|l| l.split(','))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.rsplit("::").next().unwrap_or(t).to_owned())
        .collect()
}

/// The permission that allows a command (`list_models` → `allow-list-models`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn permission_for(command: &str) -> String {
    format!("allow-{}", command.replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::*;

    const LIB_RS: &str = include_str!("lib.rs");
    const MAIN: &str = include_str!("../capabilities/default.json");
    const QUICK: &str = include_str!("../capabilities/quick.json");
    const COMPUTER: &str = include_str!("../capabilities/computer.json");
    const CONF: &str = include_str!("../tauri.conf.json");

    fn capability(text: &str) -> Value {
        serde_json::from_str(text).expect("capability JSON")
    }

    fn permissions(cap: &Value) -> Vec<String> {
        cap["permissions"]
            .as_array()
            .expect("permissions")
            .iter()
            .map(|p| p.as_str().expect("plain permission ids").to_owned())
            .collect()
    }

    /// The app-command permissions a capability grants.
    fn app_permissions(cap: &Value) -> BTreeSet<String> {
        permissions(cap)
            .into_iter()
            .filter(|p| p.starts_with("allow-"))
            .collect()
    }

    fn windows(cap: &Value) -> Vec<String> {
        cap["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .map(|w| w.as_str().expect("label").to_owned())
            .collect()
    }

    #[test]
    fn reads_the_registered_commands() {
        let cmds = commands_in(LIB_RS);
        assert!(cmds.len() > 150, "{}", cmds.len());
        for c in [
            "ping",
            "list_conversations",
            "delete_all_data",
            "install_update",
        ] {
            assert!(cmds.iter().any(|x| x == c), "{c}");
        }
        assert!(
            cmds.iter().all(|c| c
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')),
            "{cmds:?}"
        );
        assert_eq!(permission_for("list_models"), "allow-list-models");
    }

    #[test]
    fn the_main_window_may_call_every_command_and_nothing_unknown() {
        let main = capability(MAIN);
        assert_eq!(windows(&main), ["main"]);
        let want: BTreeSet<String> = commands_in(LIB_RS)
            .iter()
            .map(|c| permission_for(c))
            .collect();
        let have = app_permissions(&main);
        let missing: Vec<_> = want.difference(&have).collect();
        let unknown: Vec<_> = have.difference(&want).collect();
        assert!(
            missing.is_empty() && unknown.is_empty(),
            "capabilities/default.json is out of step with collect_commands!: missing {missing:?}, unknown {unknown:?}"
        );
    }

    #[test]
    fn small_windows_get_only_what_they_use() {
        let all: BTreeSet<String> = commands_in(LIB_RS)
            .iter()
            .map(|c| permission_for(c))
            .collect();
        let quick = capability(QUICK);
        assert_eq!(windows(&quick), ["quick"]);
        let expected: BTreeSet<String> = [
            "list_models",
            "start_conversation",
            "hide_quick_entry",
            "open_quick_entry_chat",
            "capture_screenshot",
        ]
        .iter()
        .map(|c| permission_for(c))
        .collect();
        assert_eq!(app_permissions(&quick), expected);

        let bar = capability(COMPUTER);
        assert_eq!(windows(&bar), ["computer"]);
        let expected: BTreeSet<String> = ["computer_activity", "computer_stop_all"]
            .iter()
            .map(|c| permission_for(c))
            .collect();
        assert_eq!(app_permissions(&bar), expected);
        for cap in [&quick, &bar] {
            assert!(app_permissions(cap).is_subset(&all));
            let perms = permissions(cap);
            assert!(
                !perms
                    .iter()
                    .any(|p| p.contains("close") || p.contains("minimize")),
                "{perms:?}"
            );
        }
    }

    #[test]
    fn no_window_gets_broad_core_sets_or_remote_access() {
        for cap in [MAIN, QUICK, COMPUTER].map(capability) {
            let perms = permissions(&cap);
            for p in &perms {
                assert!(
                    !p.ends_with(":default") && !p.contains("allow-all"),
                    "{p} grants more than a window needs"
                );
                assert!(
                    p.starts_with("allow-")
                        || p.starts_with("core:event:")
                        || p.starts_with("core:window:"),
                    "{p}: windows call plugins only through Oxide's own commands"
                );
            }
            assert!(
                cap.get("remote").is_none(),
                "no remote origin gets IPC (artifact frames included)"
            );
        }
    }

    #[test]
    fn the_content_security_policy_stays_tight() {
        let conf: Value = serde_json::from_str(CONF).expect("tauri.conf.json");
        let csp = conf["app"]["security"]["csp"].as_str().expect("csp string");
        let directive = |name: &str| {
            csp.split(';')
                .map(str::trim)
                .find(|d| d.split_whitespace().next() == Some(name))
                .unwrap_or_else(|| panic!("{name} missing from {csp}"))
                .to_owned()
        };
        assert_eq!(directive("default-src"), "default-src 'self'");
        assert_eq!(directive("script-src"), "script-src 'self'");
        assert_eq!(directive("object-src"), "object-src 'none'");
        assert_eq!(directive("base-uri"), "base-uri 'self'");
        assert_eq!(directive("form-action"), "form-action 'none'");
        assert!(
            !directive("connect-src").contains("http:")
                || directive("connect-src").contains("ipc.localhost")
        );
        assert!(
            !csp.contains("unsafe-eval") && !csp.contains("asset:"),
            "{csp}"
        );
        assert!(!directive("frame-src").contains('*'));
        // Generated video and audio play from blob: URLs made in the window (Phase 19).
        assert_eq!(directive("media-src"), "media-src 'self' blob:");
        assert!(
            conf["app"]["security"]
                .get("dangerousDisableAssetCspModification")
                .is_none()
        );
    }
}
