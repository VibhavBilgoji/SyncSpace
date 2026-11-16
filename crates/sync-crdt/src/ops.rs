//! Local edits of a block document. Each one returns the update to send to peers.
//!
//! Every argument is checked **before** a transaction is opened, so a failing edit changes
//! nothing. Block ids and the replica's client id come from the caller (the Wasm build has no
//! randomness): see [`crate::schema::is_valid_block_id`]. Text offsets are UTF-16 units.

use serde_json::Value;

use yrs::{Any, GetString, In, Map, MapPrelim, MapRef, Out, Text, TextPrelim, TextRef, Transact};

use crate::document::Document;
use crate::error::CrdtError;
use crate::schema::{
    BlockType, KEY_ORDER, KEY_PROPS, KEY_TEXT, KEY_TYPE, MAX_BLOCK_TEXT_UNITS, MAX_BLOCKS,
    check_block_id, check_order, check_prop,
};

fn string_value(text: &str) -> Any {
    Any::String(text.into())
}

fn prop_value(value: &Value) -> Option<Any> {
    match value {
        Value::String(text) => Some(string_value(text)),
        Value::Bool(flag) => Some(Any::Bool(*flag)),
        _ => None,
    }
}

impl Document {
    /// The map of block `id`.
    fn block_map(&self, id: &str) -> Result<MapRef, CrdtError> {
        check_block_id(id)?;
        let txn = self.doc.transact();
        match self.blocks.get(&txn, id) {
            Some(Out::YMap(map)) => Ok(map),
            _ => Err(CrdtError::BlockNotFound(id.to_string())),
        }
    }

    fn block_text(&self, map: &MapRef) -> Result<TextRef, CrdtError> {
        let txn = self.doc.transact();
        match map.get(&txn, KEY_TEXT) {
            Some(Out::YText(text)) => Ok(text),
            _ => Err(CrdtError::Invalid("block has no text".to_string())),
        }
    }

    fn block_type_of(&self, map: &MapRef) -> Result<BlockType, CrdtError> {
        let txn = self.doc.transact();
        match map.get(&txn, KEY_TYPE) {
            Some(Out::Any(Any::String(name))) => BlockType::parse(&name)
                .ok_or_else(|| CrdtError::Invalid("unknown block type".to_string())),
            _ => Err(CrdtError::Invalid("block has no type".to_string())),
        }
    }

    /// Creates a block. `order` places it (see [`crate::schema::order_between`]); `content` is its
    /// first text.
    pub fn create_block(
        &self,
        id: &str,
        block_type: BlockType,
        order: f64,
        content: &str,
    ) -> Result<Vec<u8>, CrdtError> {
        check_block_id(id)?;
        check_order(order)?;
        if content.encode_utf16().count() > MAX_BLOCK_TEXT_UNITS as usize {
            return Err(CrdtError::Limit("block text is too long".to_string()));
        }
        {
            let txn = self.doc.transact();
            if self.blocks.contains_key(&txn, id) {
                return Err(CrdtError::BlockExists(id.to_string()));
            }
            if self.blocks.len(&txn) as usize >= MAX_BLOCKS {
                return Err(CrdtError::Limit(format!("more than {MAX_BLOCKS} blocks")));
            }
        }
        let block = MapPrelim::from([
            (KEY_TYPE, In::Any(string_value(block_type.as_str()))),
            (KEY_ORDER, In::Any(Any::from(order))),
            (KEY_TEXT, In::from(TextPrelim::new(content))),
            (KEY_PROPS, In::from(MapPrelim::default())),
        ]);
        let mut txn = self.doc.transact_mut();
        self.blocks.insert(&mut txn, id, block);
        Ok(txn.encode_update_v1())
    }

    pub fn delete_block(&self, id: &str) -> Result<Vec<u8>, CrdtError> {
        self.block_map(id)?;
        let mut txn = self.doc.transact_mut();
        self.blocks.remove(&mut txn, id);
        Ok(txn.encode_update_v1())
    }

    /// Inserts `chunk` into the text of block `id` at `index`.
    pub fn insert_text(&self, id: &str, index: u32, chunk: &str) -> Result<Vec<u8>, CrdtError> {
        let text = self.block_text(&self.block_map(id)?)?;
        let added = chunk.encode_utf16().count();
        {
            let txn = self.doc.transact();
            let text_len = text.len(&txn);
            if index > text_len {
                return Err(CrdtError::OutOfRange {
                    index,
                    len: 0,
                    text_len,
                });
            }
            if text_len as usize + added > MAX_BLOCK_TEXT_UNITS as usize {
                return Err(CrdtError::Limit("block text is too long".to_string()));
            }
        }
        let mut txn = self.doc.transact_mut();
        text.insert(&mut txn, index, chunk);
        Ok(txn.encode_update_v1())
    }

    /// Deletes `len` UTF-16 units of the text of block `id`, starting at `index`.
    pub fn delete_text(&self, id: &str, index: u32, len: u32) -> Result<Vec<u8>, CrdtError> {
        let text = self.block_text(&self.block_map(id)?)?;
        {
            let txn = self.doc.transact();
            let text_len = text.len(&txn);
            if u64::from(index) + u64::from(len) > u64::from(text_len) {
                return Err(CrdtError::OutOfRange {
                    index,
                    len,
                    text_len,
                });
            }
        }
        let mut txn = self.doc.transact_mut();
        text.remove_range(&mut txn, index, len);
        Ok(txn.encode_update_v1())
    }

    /// Changes the type of block `id`. Properties that do not belong to the new type are dropped
    /// in the same transaction.
    pub fn set_type(&self, id: &str, block_type: BlockType) -> Result<Vec<u8>, CrdtError> {
        let map = self.block_map(id)?;
        let (props, foreign): (Option<MapRef>, Vec<String>) = {
            let txn = self.doc.transact();
            match map.get(&txn, KEY_PROPS) {
                Some(Out::YMap(props)) => {
                    let foreign = props
                        .keys(&txn)
                        .filter(|key| block_type.allowed_prop() != Some(*key))
                        .map(str::to_string)
                        .collect();
                    (Some(props), foreign)
                }
                _ => (None, Vec::new()),
            }
        };
        let mut txn = self.doc.transact_mut();
        map.insert(&mut txn, KEY_TYPE, string_value(block_type.as_str()));
        if let Some(props) = props {
            for key in foreign {
                props.remove(&mut txn, &key);
            }
        }
        Ok(txn.encode_update_v1())
    }

    /// Replaces the properties of block `id` with the JSON object `props_json`
    /// (for example `{"language": "rust"}`). The object is checked against the block's type.
    pub fn set_props(&self, id: &str, props_json: &str) -> Result<Vec<u8>, CrdtError> {
        let map = self.block_map(id)?;
        let block_type = self.block_type_of(&map)?;
        let parsed: Value = serde_json::from_str(props_json)
            .map_err(|_| CrdtError::InvalidProps("not valid JSON".to_string()))?;
        let Value::Object(wanted) = parsed else {
            return Err(CrdtError::InvalidProps("must be a JSON object".to_string()));
        };
        for (key, value) in &wanted {
            check_prop(block_type, key, value)?;
        }

        let existing: Option<(MapRef, Vec<String>)> = {
            let txn = self.doc.transact();
            match map.get(&txn, KEY_PROPS) {
                Some(Out::YMap(props)) => {
                    let keys = props.keys(&txn).map(str::to_string).collect();
                    Some((props, keys))
                }
                _ => None,
            }
        };

        let mut txn = self.doc.transact_mut();
        let props = match existing {
            Some((props, keys)) => {
                for key in keys.iter().filter(|key| !wanted.contains_key(*key)) {
                    props.remove(&mut txn, key);
                }
                props
            }
            None => map.insert(&mut txn, KEY_PROPS, MapPrelim::default()),
        };
        for (key, value) in &wanted {
            if let Some(value) = prop_value(value) {
                props.insert(&mut txn, key.as_str(), value);
            }
        }
        Ok(txn.encode_update_v1())
    }

    /// Moves block `id` to `order`. Two replicas that move the same block at the same time end
    /// with one of the two orders (the last write wins), never with two copies.
    pub fn move_block(&self, id: &str, order: f64) -> Result<Vec<u8>, CrdtError> {
        check_order(order)?;
        let map = self.block_map(id)?;
        let mut txn = self.doc.transact_mut();
        map.insert(&mut txn, KEY_ORDER, Any::from(order));
        Ok(txn.encode_update_v1())
    }

    /// Rewrites every order to `1.0, 2.0, 3.0, ...` in the current reading order, in one
    /// transaction. For when [`crate::schema::order_between`] reports that there is no room.
    pub fn renumber(&self) -> Vec<u8> {
        let ordered: Vec<MapRef> = {
            let txn = self.doc.transact();
            let mut found: Vec<(f64, String, MapRef)> = self
                .blocks
                .iter(&txn)
                .filter_map(|(id, value)| match value {
                    Out::YMap(map) => match map.get(&txn, KEY_ORDER) {
                        Some(Out::Any(Any::Number(order))) => {
                            Some((order.as_f64().unwrap_or(0.0), id.to_string(), map))
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            found.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            found.into_iter().map(|(_, _, map)| map).collect()
        };
        let mut txn = self.doc.transact_mut();
        for (position, map) in ordered.iter().enumerate() {
            map.insert(&mut txn, KEY_ORDER, Any::from((position + 1) as f64));
        }
        txn.encode_update_v1()
    }

    /// The text of block `id`.
    pub fn block_content(&self, id: &str) -> Result<String, CrdtError> {
        let text = self.block_text(&self.block_map(id)?)?;
        let txn = self.doc.transact();
        Ok(text.get_string(&txn))
    }
}
