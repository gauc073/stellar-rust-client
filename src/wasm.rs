use crate::error::{Result, SorobanUtilsError};
use std::path::Path;

/// Read a compiled contract `.wasm` file from disk.
///
/// Sync on purpose (matches `fs.readFileSync` in the TS source) -- confirmed
/// acceptable since this crate is used from short-lived deployment scripts,
/// not a hot path in a long-running service.
pub fn read_wasm_file(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|source| SorobanUtilsError::WasmRead {
        path: path.display().to_string(),
        source,
    })
}
