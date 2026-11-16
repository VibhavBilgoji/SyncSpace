//! Accounts and sessions (`docs/API.md` sections 1 and 2): register, log in, log out, who am I.
//!
//! A session is an HS256 JWT in the `ss_session` cookie ([`session`], [`cookie`]); passwords
//! are stored as argon2id hashes ([`password`]); [`AuthUser`] guards every route that needs a
//! signed-in user. [`AuthService`] is the one place where hashing happens, because hashing is
//! the only part of authentication that can hurt the server.

pub mod cookie;
mod extractor;
pub mod handlers;
pub mod password;
pub mod session;
pub mod validation;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::ApiError;

pub use extractor::AuthUser;

/// How many password hashes may run at the same time. Each one allocates 19 MiB, and Render's
/// free instance has 512 MB: without a limit, a few dozen parallel logins (or one attacker)
/// would get the process killed. Four hashes need about 76 MiB.
pub const MAX_CONCURRENT_HASHES: usize = 4;

/// How long a request waits for its turn before it is answered with 429.
pub const HASH_QUEUE_WAIT: Duration = Duration::from_secs(5);

/// Runs password hashing off the async threads and at most [`MAX_CONCURRENT_HASHES`] at a time.
pub struct AuthService {
    permits: Arc<Semaphore>,
    wait: Duration,
    /// A valid hash of a password nobody has. A login for an unknown email verifies against
    /// it, so that "no such user" costs as much time as "wrong password".
    dummy_hash: String,
}

impl AuthService {
    pub fn new() -> Result<Self, password::HashError> {
        Self::with_limits(MAX_CONCURRENT_HASHES, HASH_QUEUE_WAIT)
    }

    /// Smaller limits let a test exhaust the capacity without hundreds of requests.
    pub fn with_limits(max_concurrent: usize, wait: Duration) -> Result<Self, password::HashError> {
        Ok(Self {
            permits: Arc::new(Semaphore::new(max_concurrent)),
            wait,
            dummy_hash: password::hash("a password that no account has")?,
        })
    }

    /// The semaphore behind the limit, so a test can hold permits.
    pub fn semaphore(&self) -> Arc<Semaphore> {
        self.permits.clone()
    }

    async fn permit(&self) -> Result<OwnedSemaphorePermit, ApiError> {
        match tokio::time::timeout(self.wait, self.permits.clone().acquire_owned()).await {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(closed)) => Err(ApiError::internal(closed)),
            Err(_waited_too_long) => Err(ApiError::RateLimited),
        }
    }

    pub async fn hash(&self, password: String) -> Result<String, ApiError> {
        let permit = self.permit().await?;
        let work = tokio::task::spawn_blocking(move || {
            let _permit = permit; // released when the hashing is done
            password::hash(&password)
        })
        .await;
        match work {
            Ok(Ok(hash)) => Ok(hash),
            Ok(Err(error)) => Err(ApiError::internal(error)),
            Err(join_error) => Err(ApiError::internal(join_error)),
        }
    }

    pub async fn verify(&self, password: String, stored: String) -> Result<bool, ApiError> {
        let permit = self.permit().await?;
        let work = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            password::verify(&password, &stored)
        })
        .await;
        work.map_err(ApiError::internal)
    }

    /// Spends the time of a real verification on nothing. The result is always "no".
    pub async fn verify_dummy(&self, password: String) -> Result<(), ApiError> {
        self.verify(password, self.dummy_hash.clone())
            .await
            .map(|_| ())
    }
}
