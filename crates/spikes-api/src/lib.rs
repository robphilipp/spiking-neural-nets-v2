//! The protocol between the session engine and its clients (app plan 02).
//!
//! - [`protocol`]: JSON requests, responses and notices (TypeScript types generated with `ts-rs`).
//! - [`frame`]: the binary live-data frames, engine → client.
//! - [`input`]: binary sensor input batches, client → engine.

pub mod frame;
pub mod input;
pub mod protocol;

/// Protocol version; frames carry it in their header.
pub const PROTOCOL_VERSION: u16 = 1;
