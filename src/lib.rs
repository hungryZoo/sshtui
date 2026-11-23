//! Building blocks for a terminal interface that manages SSH hosts.
//! Hosts are stored in the OpenSSH configuration, and connections use the system SSH client.

pub mod app;
pub mod config;
pub mod dialog;
pub mod history;
pub mod i18n;
pub mod keys;
pub mod known_hosts;
pub mod paths;
pub mod ssh;
pub mod state;
pub mod system;
pub mod tui;
pub mod ui;
pub mod update;
pub mod validate;
