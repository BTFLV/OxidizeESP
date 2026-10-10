//! Hardware abstraction.
//!
//! [`Backend`] is the single entry point for touching serial ports and
//! devices. The production implementation ([`espflash_backend`]) is built on
//! the espflash library; a simulated implementation is available behind the
//! `mock` feature for development and tests.

use std::{path::Path, time::Duration};

use espflash::{image_format::Segment, target::ProgressCallbacks};
use serde::Serialize;
use ts_rs::TS;

use crate::{
    Result,
    model::{Chip, ConnectionOptions, FlashSize, XtalFreq},
    ports::SerialPortDescriptor,
};

pub mod espflash_backend;
#[cfg(any(test, feature = "mock"))]
pub mod mock;

/// Security configuration reported by the ROM bootloader, where supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SecurityState {
    pub secure_boot: bool,
    pub flash_encryption: bool,
    pub secure_download_mode: bool,
    pub jtag_disabled: bool,
}

/// Chip information verified by communicating with the bootloader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChipInfo {
    pub chip: Chip,
    pub chip_name: String,
    /// Silicon revision as `major.minor`, when readable.
    pub revision: Option<String>,
    /// Revision encoded as `major * 100 + minor`.
    pub revision_number: Option<u16>,
    pub crystal: XtalFreq,
    /// Flash capacity reported by the flash chip, if detection succeeded.
    pub flash_size: Option<FlashSize>,
    pub features: Vec<String>,
    pub mac_address: Option<String>,
    /// Security information is not available on every chip (the original
    /// ESP32 ROM does not implement the command).
    pub security: Option<SecurityState>,
    /// Whether the RAM flasher stub is running.
    pub stub_loaded: bool,
}

impl ChipInfo {
    pub fn flash_size_bytes(&self) -> Option<u32> {
        self.flash_size.map(FlashSize::bytes)
    }

    pub fn secure_download_mode(&self) -> bool {
        self.security.as_ref().is_some_and(|s| s.secure_download_mode)
    }
}

/// Behaviour flags for flash writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBehavior {
    /// Verify written data with a device-side MD5 checksum.
    pub verify: bool,
    /// Skip segments whose contents already match.
    pub skip_unchanged: bool,
}

impl Default for WriteBehavior {
    fn default() -> Self {
        Self { verify: true, skip_unchanged: false }
    }
}

/// Access to the serial ports and devices of the system.
pub trait Backend: Send + Sync {
    /// Whether this backend simulates hardware.
    fn is_simulated(&self) -> bool;

    /// Enumerate serial ports without opening them.
    fn list_ports(&self) -> Result<Vec<SerialPortDescriptor>>;

    /// Reset the chip into its bootloader and establish a flasher connection.
    fn connect(
        &self,
        port: &SerialPortDescriptor,
        options: &ConnectionOptions,
        behavior: WriteBehavior,
        expected_chip: Option<Chip>,
    ) -> Result<Box<dyn FlashLink>>;

    /// Open the port as a plain serial connection without resetting the chip
    /// into the bootloader.
    fn open_serial(&self, port: &SerialPortDescriptor, baud: u32) -> Result<Box<dyn SerialLink>>;
}

/// An established bootloader connection. Dropping the link closes the port
/// without resetting the chip.
pub trait FlashLink: Send {
    fn chip(&self) -> Chip;

    /// Read chip details from the device.
    fn chip_info(&mut self) -> Result<ChipInfo>;

    /// Override the flash size used for subsequent operations.
    fn set_flash_size(&mut self, size: FlashSize);

    /// Write segments to flash. espflash finishes the write by performing the
    /// configured post-operation reset.
    fn write_segments(&mut self, segments: &[Segment<'_>], progress: &mut dyn ProgressCallbacks) -> Result<()>;

    /// Read `length` bytes starting at `offset` into the file at `dest`,
    /// replacing it.
    fn read_flash(&mut self, offset: u32, length: u32, dest: &Path) -> Result<()>;

    fn erase_region(&mut self, offset: u32, length: u32) -> Result<()>;

    fn erase_all(&mut self) -> Result<()>;

    /// Device-side MD5 checksum of a flash region.
    fn checksum_md5(&mut self, offset: u32, length: u32) -> Result<u128>;

    /// Apply the configured post-operation reset.
    fn reset_after(&mut self) -> Result<()>;
}

/// A plain serial connection used by the serial monitor and for resets.
pub trait SerialLink: Send {
    /// Read available bytes. Returns `Ok(0)` when the read timed out.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    fn write_all(&mut self, data: &[u8]) -> Result<()>;

    /// Reset the chip using the DTR/RTS lines so the application restarts.
    fn reset(&mut self) -> Result<()>;

    fn set_timeout(&mut self, timeout: Duration) -> Result<()>;
}
