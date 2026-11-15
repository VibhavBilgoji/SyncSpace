//! SyncSpace API server. Axum, MongoDB and the WebSocket hub arrive in later tasks.

fn main() {
    println!("syncspace server (uses {})", sync_crdt::CRATE_NAME);
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_links_sync_crdt() {
        assert_eq!(sync_crdt::CRATE_NAME, "sync-crdt");
    }
}
