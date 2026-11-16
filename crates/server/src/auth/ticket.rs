//! WebSocket tickets (`docs/API.md` section 3, `docs/ARCHITECTURE.md` sections 2 and 5).
//!
//! A browser cannot send the session cookie to the WebSocket host (it is a different origin from
//! the web app), so it first asks `POST /auth/ws-ticket` for a short-lived ticket, and opens the
//! socket with `?ticket=...`. A ticket is an HS256 token signed with `TICKET_SECRET`, a different
//! key from the session key, so a ticket is never a session and a session is never a ticket. It
//! names the user, the scope (`doc` or `workspace`) and the id it is good for, lives 30 seconds
//! and works **once**: the id of every ticket that was used is remembered until it would have
//! expired anyway. A ticket in a URL ends up in logs and in browser history, which is why it is
//! short-lived and single-use; the request logger leaves query strings out.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::session::now;

/// How long a ticket is valid (`docs/API.md`: 30 s).
pub const TICKET_SECONDS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// A document socket; the id is the document id.
    Doc,
    /// A workspace socket; the id is the workspace id.
    Workspace,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    scope: Scope,
    id: String,
    /// The ticket's own id, remembered once it is used.
    jti: String,
    iat: u64,
    exp: u64,
}

/// A ticket that is genuine and has not expired (it may still have been used already).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub user: ObjectId,
    pub scope: Scope,
    pub id: ObjectId,
    pub jti: String,
    pub expires_at: u64,
}

/// Why a ticket was refused. The socket closes with 4001 for every variant.
#[derive(Debug, PartialEq, Eq)]
pub enum TicketError {
    Expired,
    Invalid,
}

/// A ticket for `user` to open the `scope` socket of `id`, issued at `issued_at` (Unix seconds)
/// and valid for `lifetime` seconds. Returns the token and the time it expires.
pub fn issue(
    secret: &[u8],
    user: ObjectId,
    scope: Scope,
    id: ObjectId,
    issued_at: u64,
    lifetime: u64,
) -> Result<(String, u64), jsonwebtoken::errors::Error> {
    let expires_at = issued_at + lifetime;
    let claims = Claims {
        sub: user.to_hex(),
        scope,
        id: id.to_hex(),
        jti: uuid::Uuid::new_v4().simple().to_string(),
        iat: issued_at,
        exp: expires_at,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret)?,
    )?;
    Ok((token, expires_at))
}

/// Checks the signature and the expiry (HS256 only, no clock leeway).
pub fn verify(secret: &[u8], token: &str) -> Result<Ticket, TicketError> {
    let key = DecodingKey::from_secret(secret).map_err(|_| TicketError::Invalid)?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.required_spec_claims = ["exp", "sub"].into_iter().map(String::from).collect();

    let data = decode::<Claims>(token, &key, &validation).map_err(|error| {
        if matches!(error.kind(), ErrorKind::ExpiredSignature) {
            TicketError::Expired
        } else {
            TicketError::Invalid
        }
    })?;
    let claims = data.claims;
    Ok(Ticket {
        user: ObjectId::parse_str(&claims.sub).map_err(|_| TicketError::Invalid)?,
        scope: claims.scope,
        id: ObjectId::parse_str(&claims.id).map_err(|_| TicketError::Invalid)?,
        jti: claims.jti,
        expires_at: claims.exp,
    })
}

/// The ids of tickets that were used, so that a ticket works once.
#[derive(Default)]
pub struct TicketStore {
    used: Mutex<HashMap<String, u64>>,
}

impl TicketStore {
    fn used(&self) -> MutexGuard<'_, HashMap<String, u64>> {
        self.used.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Marks `ticket` as used. `false` when it was used before. Entries whose ticket has expired
    /// are dropped on the way, because an expired ticket is refused by [`verify`] anyway.
    pub fn consume(&self, ticket: &Ticket) -> bool {
        self.consume_at(ticket, now())
    }

    pub fn consume_at(&self, ticket: &Ticket, now: u64) -> bool {
        let mut used = self.used();
        used.retain(|_, expires_at| *expires_at > now);
        used.insert(ticket.jti.clone(), ticket.expires_at).is_none()
    }

    /// How many used tickets are remembered (for tests).
    pub fn remembered(&self) -> usize {
        self.used().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = &[7u8; 32];
    const OTHER: &[u8] = &[9u8; 32];

    fn ids() -> (ObjectId, ObjectId) {
        (ObjectId::new(), ObjectId::new())
    }

    #[test]
    fn a_ticket_round_trips_and_lives_thirty_seconds() {
        let (user, doc) = ids();
        let at = now();
        let (token, expires_at) = issue(SECRET, user, Scope::Doc, doc, at, TICKET_SECONDS).unwrap();
        assert_eq!(expires_at, at + 30);
        let ticket = verify(SECRET, &token).unwrap();
        assert_eq!(
            (ticket.user, ticket.scope, ticket.id, ticket.expires_at),
            (user, Scope::Doc, doc, at + 30)
        );
        assert_eq!(ticket.jti.len(), 32);
    }

    #[test]
    fn the_scope_of_a_workspace_ticket_is_kept() {
        let (user, workspace) = ids();
        let (token, _) = issue(
            SECRET,
            user,
            Scope::Workspace,
            workspace,
            now(),
            TICKET_SECONDS,
        )
        .unwrap();
        assert_eq!(verify(SECRET, &token).unwrap().scope, Scope::Workspace);
    }

    #[test]
    fn an_expired_ticket_is_refused_with_no_leeway() {
        let (user, doc) = ids();
        let (token, _) = issue(SECRET, user, Scope::Doc, doc, now() - 31, TICKET_SECONDS).unwrap();
        assert_eq!(verify(SECRET, &token), Err(TicketError::Expired));
        // One second left is still good.
        let (token, _) = issue(SECRET, user, Scope::Doc, doc, now() - 28, TICKET_SECONDS).unwrap();
        assert!(verify(SECRET, &token).is_ok());
    }

    #[test]
    fn a_ticket_signed_with_another_key_or_tampered_with_is_refused() {
        let (user, doc) = ids();
        let (token, _) = issue(OTHER, user, Scope::Doc, doc, now(), TICKET_SECONDS).unwrap();
        assert_eq!(verify(SECRET, &token), Err(TicketError::Invalid));

        let (token, _) = issue(SECRET, user, Scope::Doc, doc, now(), TICKET_SECONDS).unwrap();
        let mut parts: Vec<&str> = token.split('.').collect();
        parts[2] = "AAAA";
        assert_eq!(verify(SECRET, &parts.join(".")), Err(TicketError::Invalid));
        for junk in ["", "a.b.c", "not a token", "...."] {
            assert_eq!(verify(SECRET, junk), Err(TicketError::Invalid), "{junk:?}");
        }
    }

    #[test]
    fn a_session_token_is_not_a_ticket() {
        // The two keys differ (the config enforces it), so a session never verifies as a ticket.
        let session = super::super::session::issue(OTHER, ObjectId::new(), now()).unwrap();
        assert_eq!(verify(SECRET, &session), Err(TicketError::Invalid));
        // Even with the same key, a session lacks the scope and id of a ticket.
        let session = super::super::session::issue(SECRET, ObjectId::new(), now()).unwrap();
        assert_eq!(verify(SECRET, &session), Err(TicketError::Invalid));
    }

    #[test]
    fn a_ticket_works_once() {
        let (user, doc) = ids();
        let (token, _) = issue(SECRET, user, Scope::Doc, doc, now(), TICKET_SECONDS).unwrap();
        let ticket = verify(SECRET, &token).unwrap();
        let store = TicketStore::default();
        assert!(store.consume(&ticket));
        assert!(!store.consume(&ticket), "the second use is refused");

        // Another ticket for the same user and document is a different ticket.
        let (second, _) = issue(SECRET, user, Scope::Doc, doc, now(), TICKET_SECONDS).unwrap();
        assert!(store.consume(&verify(SECRET, &second).unwrap()));
    }

    #[test]
    fn used_tickets_are_forgotten_once_they_would_have_expired() {
        let (user, doc) = ids();
        let store = TicketStore::default();
        let at = 1_000_000;
        for _ in 0..5 {
            let (token, _) = issue(SECRET, user, Scope::Doc, doc, at, TICKET_SECONDS).unwrap();
            // `verify` checks against the real clock, so read the claims through a fresh
            // ticket built by hand for the store.
            let _ = token;
            let ticket = Ticket {
                user,
                scope: Scope::Doc,
                id: doc,
                jti: uuid::Uuid::new_v4().simple().to_string(),
                expires_at: at + TICKET_SECONDS,
            };
            assert!(store.consume_at(&ticket, at));
        }
        assert_eq!(store.remembered(), 5);

        // A use after they have all expired clears them and remembers only itself.
        let late = Ticket {
            user,
            scope: Scope::Doc,
            id: doc,
            jti: "late".into(),
            expires_at: at + 100 + TICKET_SECONDS,
        };
        assert!(store.consume_at(&late, at + 100));
        assert_eq!(store.remembered(), 1);
    }
}
