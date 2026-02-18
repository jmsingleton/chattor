//! chattor library
//!
//! Core modules for the chattor application.

pub mod error;
pub mod app;
pub mod cli;
pub mod config;
pub mod crypto;
pub mod db;
pub mod tor;
pub mod protocol;
pub mod net;
pub mod notifications;
pub mod presence;
pub mod ui;

pub use error::{Result, TorrentChatError};
