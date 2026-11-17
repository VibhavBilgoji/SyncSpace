//! JavaScript API for the browser, built with `wasm-bindgen`.
//!
//! Thin wrappers over [`SpikeDoc`] (the 0.3 spike, kept until the hub and the editor replace it)
//! and [`Document`] (the block document of task 2.1): all behaviour lives in those types, so
//! native and Wasm cannot drift. Local edits return their update bytes instead of using a
//! callback; the page sends them on.

use wasm_bindgen::prelude::*;

use crate::parity::{block_doc_from_fixture, doc_from_fixture};
use crate::{
    BlockType, Document, SpikeDoc, awareness_entries, awareness_frame, order_between, update_frame,
};

/// A client id from JavaScript: an integer below 2^53. The range check also rejects NaN and
/// infinities.
fn parse_client_id(value: f64) -> Result<u64, JsError> {
    if (0.0..9_007_199_254_740_992.0).contains(&value) && value.fract() == 0.0 {
        Ok(value as u64)
    } else {
        Err(JsError::new("client id must be an integer below 2^53"))
    }
}

fn parse_block_type(name: &str) -> Result<BlockType, JsError> {
    BlockType::parse(name).ok_or_else(|| JsError::new("unknown block type"))
}

#[wasm_bindgen]
pub struct WasmDoc {
    inner: SpikeDoc,
}

#[wasm_bindgen]
impl WasmDoc {
    /// `client_id` must be unique among live replicas. Pass a random integer below 2^53
    /// (for example from `crypto.getRandomValues`); Wasm has no randomness of its own.
    #[wasm_bindgen(constructor)]
    pub fn new(client_id: f64) -> Result<WasmDoc, JsError> {
        Ok(WasmDoc {
            inner: SpikeDoc::new(parse_client_id(client_id)?)?,
        })
    }

    /// Replays a parity fixture (see `tests/fixtures/README.md`).
    #[wasm_bindgen(js_name = fromFixture)]
    pub fn from_fixture(bytes: &[u8]) -> Result<WasmDoc, JsError> {
        Ok(WasmDoc {
            inner: doc_from_fixture(bytes)?,
        })
    }

    /// Inserts at a UTF-16 index and returns the update to send to the server.
    pub fn insert(&self, index: u32, chunk: &str) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.insert(index, chunk)?)
    }

    /// Deletes `len` UTF-16 units at `index` and returns the update to send.
    pub fn delete(&self, index: u32, len: u32) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.delete(index, len)?)
    }

    pub fn text(&self) -> String {
        self.inner.text()
    }

    #[wasm_bindgen(js_name = encodeStateAsUpdate)]
    pub fn encode_state_as_update(&self) -> Vec<u8> {
        self.inner.encode_state_as_update()
    }

    /// Applies an update from the server. Throws on invalid bytes.
    #[wasm_bindgen(js_name = applyUpdate)]
    pub fn apply_update(&self, update: &[u8]) -> Result<(), JsError> {
        Ok(self.inner.apply_update(update)?)
    }
}

/// The block document for the editor (`docs/ARCHITECTURE.md` section 2, task 2.1).
///
/// Every edit returns the update bytes; the page sends them to the server as an Update frame
/// (see `updateFrame`). After `applyUpdate` or `handleMessage` the page calls `blocksJson()` to
/// re-render. Block ids are 12 letters or digits that the page creates (use
/// `crypto.getRandomValues`); the client id must be unique among live replicas.
#[wasm_bindgen]
pub struct WasmDocument {
    inner: Document,
}

#[wasm_bindgen]
impl WasmDocument {
    #[wasm_bindgen(constructor)]
    pub fn new(client_id: f64) -> Result<WasmDocument, JsError> {
        Ok(WasmDocument {
            inner: Document::new(parse_client_id(client_id)?)?,
        })
    }

    /// Replays a block parity fixture (see `tests/fixtures/README.md`).
    #[wasm_bindgen(js_name = fromFixture)]
    pub fn from_fixture(bytes: &[u8]) -> Result<WasmDocument, JsError> {
        Ok(WasmDocument {
            inner: block_doc_from_fixture(bytes)?,
        })
    }

    #[wasm_bindgen(js_name = createBlock)]
    pub fn create_block(
        &self,
        id: &str,
        block_type: &str,
        order: f64,
        content: &str,
    ) -> Result<Vec<u8>, JsError> {
        Ok(self
            .inner
            .create_block(id, parse_block_type(block_type)?, order, content)?)
    }

    #[wasm_bindgen(js_name = deleteBlock)]
    pub fn delete_block(&self, id: &str) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.delete_block(id)?)
    }

    /// Inserts at a UTF-16 index of the block's text.
    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(&self, id: &str, index: u32, chunk: &str) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.insert_text(id, index, chunk)?)
    }

    /// Deletes `len` UTF-16 units of the block's text at `index`.
    #[wasm_bindgen(js_name = deleteText)]
    pub fn delete_text(&self, id: &str, index: u32, len: u32) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.delete_text(id, index, len)?)
    }

    #[wasm_bindgen(js_name = setType)]
    pub fn set_type(&self, id: &str, block_type: &str) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.set_type(id, parse_block_type(block_type)?)?)
    }

    /// Replaces the block's properties with a JSON object such as `{"language":"rust"}`.
    #[wasm_bindgen(js_name = setProps)]
    pub fn set_props(&self, id: &str, props_json: &str) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.set_props(id, props_json)?)
    }

    #[wasm_bindgen(js_name = moveBlock)]
    pub fn move_block(&self, id: &str, order: f64) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.move_block(id, order)?)
    }

    /// Rewrites every order to 1, 2, 3, ... for when `orderBetween` reports no room.
    pub fn renumber(&self) -> Vec<u8> {
        self.inner.renumber()
    }

    /// The text of one block.
    #[wasm_bindgen(js_name = blockContent)]
    pub fn block_content(&self, id: &str) -> Result<String, JsError> {
        Ok(self.inner.block_content(id)?)
    }

    /// The blocks in reading order, as a JSON array of `Block` (see `docs/API.md` section 4).
    #[wasm_bindgen(js_name = blocksJson)]
    pub fn blocks_json(&self) -> String {
        self.inner.blocks_json()
    }

    #[wasm_bindgen(js_name = stateVector)]
    pub fn state_vector(&self) -> Vec<u8> {
        self.inner.state_vector()
    }

    #[wasm_bindgen(js_name = encodeStateAsUpdate)]
    pub fn encode_state_as_update(&self) -> Vec<u8> {
        self.inner.encode_state_as_update()
    }

    /// What a peer with this state vector is missing.
    #[wasm_bindgen(js_name = encodeStateAsUpdateFrom)]
    pub fn encode_state_as_update_from(&self, state_vector: &[u8]) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.encode_state_as_update_from(state_vector)?)
    }

    /// Applies an update from the server. Throws on invalid bytes.
    #[wasm_bindgen(js_name = applyUpdate)]
    pub fn apply_update(&self, update: &[u8]) -> Result<(), JsError> {
        Ok(self.inner.apply_update(update)?)
    }

    /// The first frame to send after connecting: "this is what I have, send me the rest".
    #[wasm_bindgen(js_name = syncStep1Frame)]
    pub fn sync_step1_frame(&self) -> Vec<u8> {
        self.inner.sync_step1_frame()
    }

    /// Handles one binary frame from the server and returns the frame to send back (empty when
    /// there is none). Throws on a frame that cannot be handled.
    #[wasm_bindgen(js_name = handleMessage)]
    pub fn handle_message(&self, frame: &[u8]) -> Result<Vec<u8>, JsError> {
        Ok(self.inner.handle_message(frame)?.reply.unwrap_or_default())
    }
}

/// The order for a block between two neighbours (`docs/API.md`, move rule). Pass `undefined` for
/// a missing neighbour. Throws when there is no room between them: call `renumber()` first.
#[wasm_bindgen(js_name = orderBetween)]
pub fn order_between_js(before: Option<f64>, after: Option<f64>) -> Result<f64, JsError> {
    Ok(order_between(before, after)?)
}

/// An awareness frame announcing `json` as the state of this replica (`"null"` = left). Throws on
/// invalid JSON, a state over 1 KiB or a bad client id.
#[wasm_bindgen(js_name = awarenessFrame)]
pub fn awareness_frame_js(client_id: f64, clock: u32, json: &str) -> Result<Vec<u8>, JsError> {
    Ok(awareness_frame(parse_client_id(client_id)?, clock, json)?)
}

/// The entries of an awareness frame as JSON: `[{"client_id":1,"clock":2,"state":{...}|null}]`.
#[wasm_bindgen(js_name = decodeAwareness)]
pub fn decode_awareness_js(frame: &[u8]) -> Result<String, JsError> {
    let entries = awareness_entries(frame)?;
    let list: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            let state = e
                .state
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
            serde_json::json!({"client_id": e.client_id, "clock": e.clock, "state": state})
        })
        .collect();
    Ok(serde_json::Value::Array(list).to_string())
}

/// Wraps an update in an Update frame, ready to send over the document socket.
#[wasm_bindgen(js_name = updateFrame)]
pub fn update_frame_js(update: &[u8]) -> Vec<u8> {
    update_frame(update)
}
