//! Domain types shared across Oxide.
//!
//! Phase 2 fills this crate with `Conversation`, `Message`, `ContentBlock` and friends.

/// Product name shown in the UI and window titles.
pub const APP_NAME: &str = "Oxide";

/// Version of the core crate (kept in lockstep with the workspace version).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_oxide() {
        assert_eq!(APP_NAME, "Oxide");
    }

    #[test]
    fn version_is_semver_like() {
        assert_eq!(version().split('.').count(), 3);
    }
}
