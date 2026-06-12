#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
pub mod fs;
#[cfg(all(feature = "indexeddb", target_arch = "wasm32"))]
pub mod indexeddb;
pub mod memory;
