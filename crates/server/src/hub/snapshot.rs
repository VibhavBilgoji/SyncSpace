//! The snapshot worker (`docs/ARCHITECTURE.md` section 3.3): every few seconds, every document that
//! changed since its last snapshot is written to MongoDB. The final write when the last socket of
//! a document leaves, and the write on shutdown, are in [`super::Hub::release`] and
//! [`super::Hub::shutdown`].

use std::sync::Arc;
use std::time::Duration;

use mongodb::Database;
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};

use super::Hub;

/// Starts the worker. Abort the handle to stop it.
pub fn spawn(hub: Arc<Hub>, db: Database, every: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = interval(every);
        // A slow write delays the next tick instead of stacking up ticks behind it.
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tick.tick().await; // the first tick fires at once
        loop {
            tick.tick().await;
            hub.flush_all(&db).await;
        }
    })
}
