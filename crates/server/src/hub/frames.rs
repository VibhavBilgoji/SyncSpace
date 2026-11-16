//! The JSON text frames of the document socket (`docs/API.md` section 3). Binary frames are the
//! y-sync protocol and are handled by `sync-crdt`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A frame the server sends as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerFrame {
    /// The answer to a client `PING`; the client shows the round-trip time.
    Pong { t: i64 },
    /// Something the server refused; the connection stays open.
    Error {
        code: SocketErrorCode,
        message: String,
    },
    /// How many sockets are open on the document ("Active Sockets: N").
    PresenceCount { active_sockets: u32 },
    /// The document was written to MongoDB as this revision ("rev_N · Saved to MongoDB").
    SnapshotSaved { revision: i64, saved_at: String },
}

/// A frame the client sends as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientFrame {
    /// Every 20 seconds; `t` is the client's clock and comes back in the `PONG`.
    Ping { t: i64 },
}

/// The `code` of a socket `ERROR` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SocketErrorCode {
    /// A viewer sent a change; it was dropped.
    ReadOnly,
    /// Too many frames; this one was dropped.
    RateLimited,
    /// A frame or the document is over its size limit.
    TooLarge,
}

impl ServerFrame {
    pub fn error(code: SocketErrorCode, message: &str) -> Self {
        Self::Error {
            code,
            message: message.to_string(),
        }
    }

    /// The frame as the text that goes on the wire.
    pub fn to_text(&self) -> String {
        // Serialising these plain enums cannot fail; the fallback keeps the function total.
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn server_frames_have_the_documented_wire_shape() {
        let value = |frame: ServerFrame| serde_json::to_value(frame).unwrap();
        assert_eq!(
            value(ServerFrame::Pong { t: 5 }),
            json!({"type": "PONG", "t": 5})
        );
        assert_eq!(
            value(ServerFrame::PresenceCount { active_sockets: 3 }),
            json!({"type": "PRESENCE_COUNT", "active_sockets": 3})
        );
        assert_eq!(
            value(ServerFrame::SnapshotSaved {
                revision: 4,
                saved_at: "2026-11-17T01:02:03.004Z".into()
            }),
            json!({"type": "SNAPSHOT_SAVED", "revision": 4, "saved_at": "2026-11-17T01:02:03.004Z"})
        );
        assert_eq!(
            value(ServerFrame::error(
                SocketErrorCode::ReadOnly,
                "viewers cannot edit"
            )),
            json!({"type": "ERROR", "code": "READ_ONLY", "message": "viewers cannot edit"})
        );
        assert_eq!(
            serde_json::to_value(SocketErrorCode::RateLimited).unwrap(),
            json!("RATE_LIMITED")
        );
        assert_eq!(
            serde_json::to_value(SocketErrorCode::TooLarge).unwrap(),
            json!("TOO_LARGE")
        );
    }

    #[test]
    fn client_frames_parse_and_everything_else_is_refused() {
        let ping: ClientFrame = serde_json::from_str(r#"{"type":"PING","t":42}"#).unwrap();
        assert_eq!(ping, ClientFrame::Ping { t: 42 });
        for bad in [
            "",
            "nope",
            "{}",
            r#"{"type":"PONG","t":1}"#,
            r#"{"type":"PING"}"#,
            r#"{"type":"PING","t":"x"}"#,
            r#"{"type":"ping","t":1}"#,
        ] {
            assert!(serde_json::from_str::<ClientFrame>(bad).is_err(), "{bad:?}");
        }
    }
}
