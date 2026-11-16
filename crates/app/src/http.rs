//! The loopback HTTP client the callers here use.
//!
//! The client itself is the core's (`pai_core::http`, its header carries the
//! why: a hand-rolled loopback client, no TLS, no async runtime). It moved there
//! with the save leg of the slot snapshots (`pai_core::slot_save`), which the
//! instance's own stop reaches without this crate. This module keeps the name
//! the sources here use.

pub use pai_core::http::{request, Response, CONNECT_TIMEOUT};
