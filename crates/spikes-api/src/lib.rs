//! The protocol between the session engine and its clients (app plan 02).
//!
//! So far: the binary live-data [`frame`] format. The JSON requests, responses and notices, with generated
//! TypeScript types, follow in phase A2.

pub mod frame;

/// Protocol version; frames carry it in their header.
pub const PROTOCOL_VERSION: u16 = 1;
