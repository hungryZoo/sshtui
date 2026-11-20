//! Building blocks for a terminal interface that manages SSH hosts.
//! Hosts are stored in the OpenSSH configuration, and connections use the system SSH client.

pub mod app;
pub mod config;
pub mod dialog;
pub mod keys;
pub mod paths;
pub mod ssh;
pub mod tui;
pub mod ui;
pub mod validate;
