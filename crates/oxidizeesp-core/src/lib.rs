//! Core of OxidizeESP: hardware access through espflash, device sessions,
//! long-running operations and file formats.
//!
//! This crate has no dependency on the user interface. The desktop
//! application translates frontend requests into calls on [`service::Services`].

pub mod backup;
pub mod error;
pub mod fsutil;
pub mod hardware;
pub mod image;
pub mod job;
pub mod model;
pub mod monitor;
pub mod ops;
pub mod partitions;
pub mod ports;
pub mod region;
pub mod service;
pub mod session;
pub mod settings;

#[cfg(test)]
mod testutil;

pub use error::{Error, ErrorKind, Result};
