//! Reading blocks out of a document: `blocks(doc) -> Vec<Block>` sorted by `(order, block_id)`.

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value};

use yrs::{Any, GetString, Map, MapRef, Out, ReadTxn, Text, Transact};

use crate::document::Document;
use crate::schema::{
    BlockType, KEY_ORDER, KEY_PROPS, KEY_TEXT, KEY_TYPE, MAX_BLOCK_TEXT_UNITS, check_prop_value,
};

/// One block as the API and the editor see it. The serde names are the wire names of
/// `docs/API.md` section 4 (`api_types::Block` in the server crate has the same shape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub block_id: String,
    #[serde(rename = "type")]
    pub block_type: BlockType,
    /// Plain text.
    pub content: String,
    /// `language` for CODE, `checked` for CHECKLIST, `tone` for CALLOUT.
    pub properties: JsonMap<String, Value>,
    pub order: f64,
}

/// Reads one block, strictly: any field that is missing, unknown or of the wrong kind is an
/// error. The structure check uses the error to reject an update; the projection skips blocks
/// that fail.
pub(crate) fn parse_block<T: ReadTxn>(txn: &T, id: &str, map: &MapRef) -> Result<Block, String> {
    let mut block_type = None;
    let mut order = None;
    let mut content = None;
    let mut props_map = None;

    for (key, value) in map.iter(txn) {
        match (key, value) {
            (KEY_TYPE, Out::Any(Any::String(name))) => {
                block_type = Some(BlockType::parse(&name).ok_or("unknown block type")?);
            }
            (KEY_ORDER, Out::Any(Any::Number(number))) => {
                let value = number.as_f64().ok_or("order is not a number")?;
                if !value.is_finite() {
                    return Err("order is not finite".to_string());
                }
                order = Some(value);
            }
            (KEY_TEXT, Out::YText(text)) => {
                if text.len(txn) > MAX_BLOCK_TEXT_UNITS {
                    return Err("text is too long".to_string());
                }
                content = Some(text.get_string(txn));
            }
            (KEY_PROPS, Out::YMap(props)) => props_map = Some(props),
            _ => return Err(format!("unexpected or malformed field `{key}`")),
        }
    }

    let block_type = block_type.ok_or("missing type")?;
    let order = order.ok_or("missing order")?;
    let content = content.ok_or("missing text")?;

    let mut properties = JsonMap::new();
    if let Some(props) = props_map {
        for (key, value) in props.iter(txn) {
            let value = match value {
                Out::Any(Any::String(text)) => Value::String(text.to_string()),
                Out::Any(Any::Bool(flag)) => Value::Bool(flag),
                _ => return Err(format!("property `{key}` has an unsupported value")),
            };
            check_prop_value(key, &value).map_err(|error| error.to_string())?;
            // A property of another type (left by a concurrent change of type) is valid but not
            // shown: the projection is a function of the state, so every replica shows the same.
            if block_type.allowed_prop() == Some(key) {
                properties.insert(key.to_string(), value);
            }
        }
    }

    Ok(Block {
        block_id: id.to_string(),
        block_type,
        content,
        properties,
        order,
    })
}

/// Every well-formed block of `blocks`, sorted by `(order, block_id)`.
pub(crate) fn collect_blocks<T: ReadTxn>(txn: &T, blocks: &MapRef) -> Vec<Block> {
    let mut found: Vec<Block> = blocks
        .iter(txn)
        .filter_map(|(id, value)| match value {
            Out::YMap(map) => parse_block(txn, id, &map).ok(),
            _ => None,
        })
        .collect();
    found.sort_by(|a, b| {
        a.order
            .total_cmp(&b.order)
            .then_with(|| a.block_id.cmp(&b.block_id))
    });
    found
}

impl Document {
    /// The blocks in reading order. A block that does not follow the schema (which the structure
    /// check keeps out of any document that went through [`Document::apply_update`]) is skipped.
    pub fn blocks(&self) -> Vec<Block> {
        let txn = self.doc.transact();
        collect_blocks(&txn, &self.blocks)
    }

    /// [`Document::blocks`] as the JSON array the editor reads.
    pub fn blocks_json(&self) -> String {
        // Serialising plain structs and JSON maps cannot fail; the fallback keeps the function
        // total without an `unwrap`.
        serde_json::to_string(&self.blocks()).unwrap_or_else(|_| "[]".to_string())
    }
}
