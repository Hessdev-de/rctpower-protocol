//! rctpower-protocol — Rust implementation of the RCT Power serial protocol.
//!
//! Derived from python-rctclient (GPL-3.0-only, pob90/svalouch) and
//! rctpower_writesupport (MIT, do-gooder). See NOTICE.

pub mod client;
#[cfg(feature = "async")]
pub mod async_client;
pub mod codec;
pub mod error;
pub mod frame;
pub mod registry;
pub mod types;
pub mod writable;

pub use error::RctError;
