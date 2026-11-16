//! The block schema of `docs/DATA_MODEL.md` section 6, and the rules a block must follow.
//!
//! ```text
//! root Y.Map "blocks": block_id -> Y.Map { type: string, order: f64, text: Y.Text, props: Y.Map }
//! root Y.Map "meta":   { schema: 1 }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::CrdtError;

pub const BLOCKS_ROOT: &str = "blocks";
pub const META_ROOT: &str = "meta";
pub const SCHEMA_VERSION: f64 = 1.0;

pub const KEY_TYPE: &str = "type";
pub const KEY_ORDER: &str = "order";
pub const KEY_TEXT: &str = "text";
pub const KEY_PROPS: &str = "props";
pub const KEY_SCHEMA: &str = "schema";

/// Most blocks in one document. A hostile update cannot grow the document past this.
pub const MAX_BLOCKS: usize = 2000;
/// Longest text of one block, in UTF-16 units.
pub const MAX_BLOCK_TEXT_UNITS: u32 = 100_000;
pub const BLOCK_ID_LEN: usize = 12;
pub const MAX_LANGUAGE_CHARS: usize = 32;
/// Orders closer than this are treated as having no room between them (`docs/API.md`, move rule).
pub const MIN_ORDER_GAP: f64 = 1e-9;
/// The tones a CALLOUT block may have.
pub const TONES: [&str; 4] = ["info", "success", "warning", "danger"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    #[serde(rename = "PARAGRAPH")]
    Paragraph,
    #[serde(rename = "HEADING_1")]
    Heading1,
    #[serde(rename = "HEADING_2")]
    Heading2,
    #[serde(rename = "CODE")]
    Code,
    #[serde(rename = "CHECKLIST")]
    Checklist,
    #[serde(rename = "CALLOUT")]
    Callout,
}

impl BlockType {
    pub const ALL: [BlockType; 6] = [
        Self::Paragraph,
        Self::Heading1,
        Self::Heading2,
        Self::Code,
        Self::Checklist,
        Self::Callout,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "PARAGRAPH",
            Self::Heading1 => "HEADING_1",
            Self::Heading2 => "HEADING_2",
            Self::Code => "CODE",
            Self::Checklist => "CHECKLIST",
            Self::Callout => "CALLOUT",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == name)
    }

    /// The one property a block of this type may carry, if any.
    pub fn allowed_prop(self) -> Option<&'static str> {
        match self {
            Self::Code => Some("language"),
            Self::Checklist => Some("checked"),
            Self::Callout => Some("tone"),
            _ => None,
        }
    }
}

/// A block id is 12 ASCII letters or digits (created by the client, `DATA_MODEL.md` section 6).
pub fn is_valid_block_id(id: &str) -> bool {
    id.len() == BLOCK_ID_LEN && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn check_block_id(id: &str) -> Result<(), CrdtError> {
    if is_valid_block_id(id) {
        Ok(())
    } else {
        Err(CrdtError::InvalidBlockId)
    }
}

pub fn check_order(order: f64) -> Result<(), CrdtError> {
    if order.is_finite() {
        Ok(())
    } else {
        Err(CrdtError::InvalidOrder)
    }
}

/// Checks one property of a block of `block_type`, for a local edit: the property must belong to
/// the type and have a valid value.
pub fn check_prop(block_type: BlockType, key: &str, value: &Value) -> Result<(), CrdtError> {
    if block_type.allowed_prop() != Some(key) {
        return Err(CrdtError::InvalidProps(format!(
            "{key}: not allowed on {}",
            block_type.as_str()
        )));
    }
    check_prop_value(key, value)
}

/// Checks that `key` is a known property and `value` is a valid value for it, without asking
/// which block it is on. Incoming updates are checked this way: two people can change a block's
/// type and its properties at the same moment (a checkbox ticked while the block becomes a
/// heading), and the merged block then holds a property of its old type. That is a legitimate
/// result, so it is accepted; the projection only shows the properties that belong to the type.
pub fn check_prop_value(key: &str, value: &Value) -> Result<(), CrdtError> {
    let bad = |why: &str| Err(CrdtError::InvalidProps(format!("{key}: {why}")));
    match (key, value) {
        ("language", Value::String(s)) if s.chars().count() <= MAX_LANGUAGE_CHARS => Ok(()),
        ("language", _) => bad("must be a string of at most 32 characters"),
        ("checked", Value::Bool(_)) => Ok(()),
        ("checked", _) => bad("must be true or false"),
        ("tone", Value::String(s)) if TONES.contains(&s.as_str()) => Ok(()),
        ("tone", _) => bad("must be one of info, success, warning, danger"),
        _ => bad("unknown property"),
    }
}

/// Checks every property of a block of `block_type`.
pub fn check_props(block_type: BlockType, props: &Map<String, Value>) -> Result<(), CrdtError> {
    props
        .iter()
        .try_for_each(|(key, value)| check_prop(block_type, key, value))
}

/// The order for a block that goes between two neighbours (`docs/API.md`, move rule): `1.0` when
/// there are none, `before + 1.0` at the tail, `after / 2.0` at the head, the midpoint between
/// two. When the two are closer than [`MIN_ORDER_GAP`] there is no room and the caller renumbers.
pub fn order_between(before: Option<f64>, after: Option<f64>) -> Result<f64, CrdtError> {
    let order = match (before, after) {
        (None, None) => 1.0,
        (Some(before), None) => before + 1.0,
        (None, Some(after)) => after / 2.0,
        (Some(before), Some(after)) => {
            if (after - before).abs() < MIN_ORDER_GAP {
                return Err(CrdtError::NoRoom);
            }
            (before + after) / 2.0
        }
    };
    check_order(order)?;
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_ids_are_twelve_letters_or_digits() {
        assert!(is_valid_block_id("aB3dE6gH9jKl"));
        for bad in [
            "",
            "short",
            "aB3dE6gH9jKlM",
            "aB3dE6gH9j-l",
            "aB3dE6gH9jé",
            "aB3dE6gH9j l",
        ] {
            assert!(!is_valid_block_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn block_type_names_round_trip() {
        for block_type in BlockType::ALL {
            assert_eq!(BlockType::parse(block_type.as_str()), Some(block_type));
            let json = serde_json::to_string(&block_type).unwrap();
            assert_eq!(json, format!("\"{}\"", block_type.as_str()));
        }
        assert_eq!(BlockType::parse("paragraph"), None);
        assert_eq!(BlockType::parse("HEADING_3"), None);
    }

    #[test]
    fn props_are_checked_per_block_type() {
        let ok = |t, k: &str, v: Value| check_prop(t, k, &v).is_ok();
        assert!(ok(BlockType::Code, "language", json!("rust")));
        assert!(ok(BlockType::Checklist, "checked", json!(true)));
        assert!(ok(BlockType::Callout, "tone", json!("warning")));
        // Wrong type of block, wrong value kind, unknown value, too long.
        assert!(!ok(BlockType::Paragraph, "language", json!("rust")));
        assert!(!ok(BlockType::Code, "checked", json!(true)));
        assert!(!ok(BlockType::Checklist, "checked", json!("yes")));
        assert!(!ok(BlockType::Callout, "tone", json!("purple")));
        assert!(!ok(BlockType::Code, "language", json!("x".repeat(33))));
        assert!(ok(BlockType::Code, "language", json!("x".repeat(32))));
        assert!(!ok(BlockType::Code, "other", json!("x")));
    }

    #[test]
    fn order_between_follows_the_move_rule() {
        assert_eq!(order_between(None, None).unwrap(), 1.0);
        assert_eq!(order_between(Some(3.0), None).unwrap(), 4.0);
        assert_eq!(order_between(None, Some(3.0)).unwrap(), 1.5);
        assert_eq!(order_between(Some(1.0), Some(2.0)).unwrap(), 1.5);
        assert_eq!(order_between(Some(2.0), Some(1.0)).unwrap(), 1.5);
        assert_eq!(
            order_between(Some(1.0), Some(1.0 + 1e-10)),
            Err(CrdtError::NoRoom)
        );
        assert_eq!(
            order_between(Some(f64::NAN), None),
            Err(CrdtError::InvalidOrder)
        );
        assert_eq!(
            order_between(None, Some(f64::INFINITY)),
            Err(CrdtError::InvalidOrder)
        );
    }
}
