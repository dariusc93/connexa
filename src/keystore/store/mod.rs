#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
pub mod fs;
pub mod memory;
