//! Shared CRDT logic for SyncSpace.
//!
//! Built natively for the server and to `wasm32-unknown-unknown` for the browser.
//! The Yrs-based implementation arrives in roadmap task 0.3.

/// Crate name, used by the server to confirm the dependency is wired.
pub const CRATE_NAME: &str = "sync-crdt";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_builds_and_links() {
        assert_eq!(CRATE_NAME, "sync-crdt");
    }
}
