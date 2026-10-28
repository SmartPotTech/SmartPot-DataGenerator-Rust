//! Biblioteca principal de SmartPot DataGenerator en Rust.
#![deny(unsafe_code)]

pub mod api;
pub mod config;
pub mod device;
pub mod domain;
pub mod pots;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
