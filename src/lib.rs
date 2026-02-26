//! chattor library
//!
//! Core modules for the chattor application.

pub mod app;
pub mod cli;
pub mod client;
pub mod config;
pub mod crypto;
pub mod daemon;
pub mod db;
pub mod error;
pub mod handlers;
pub mod mcp;
pub mod net;
pub mod notifications;
pub mod presence;
pub mod protocol;
pub mod tor;
pub mod ui;

pub use error::{ChattorError, Result};
