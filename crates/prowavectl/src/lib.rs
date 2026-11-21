//! prowavectl: command-line client and MCP server for Pro Wave's control API.
//!
//! The app listens on a Unix socket; this crate talks to it. Nothing here ever starts the app.

pub mod args;
pub mod cli;
pub mod client;
pub mod mcp;
pub mod methods;
