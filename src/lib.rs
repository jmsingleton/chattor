//! torrent-chat library
//!
//! Core modules for the torrent-chat application.

pub mod error;
pub mod app;
pub mod cli;
pub mod config;
pub mod crypto;
pub mod db;
pub mod tor;
pub mod protocol;
pub mod net;
pub mod ui;

pub use error::{Result, TorrentChatError};
