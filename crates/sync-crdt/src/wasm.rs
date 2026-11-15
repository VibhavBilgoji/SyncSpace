//! JavaScript API for the browser, built with `wasm-bindgen`.
//!
//! A thin wrapper over [`SpikeDoc`]: all behaviour lives there, so native and Wasm cannot drift.
//! Local edits return their update bytes instead of using a callback; the page sends them on.

use wasm_bindgen::prelude::*;

use crate::SpikeDoc;
use crate::parity::doc_from_fixture;

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
        // The range check also rejects NaN and infinities.
        if !((0.0..9_007_199_254_740_992.0).contains(&client_id) && client_id.fract() == 0.0) {
            return Err(JsError::new("client id must be an integer below 2^53"));
        }
        Ok(WasmDoc {
            inner: SpikeDoc::new(client_id as u64)?,
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
