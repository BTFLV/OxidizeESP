//! Error model shared by every layer of the application.
//!
//! Hardware and file errors are classified into a small set of
//! [`ErrorKind`]s so the user interface can present actionable messages
//! instead of raw library diagnostics. The original diagnostic is retained in
//! [`Error::detail`] for users who want the technical background.

use std::{error::Error as StdError, io, path::Path};

use serde::Serialize;
use ts_rs::TS;

/// Broad category of a failure, used by the frontend to choose wording,
/// icons and recovery hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorKind {
    /// The device did not respond or the bootloader could not be entered.
    Connection,
    /// The serial port disappeared, usually because the device was unplugged.
    Disconnected,
    /// The serial port is used by another process or another operation.
    PortBusy,
    /// The operating system denied access to the serial port or a file.
    Permission,
    /// The detected hardware is not supported by espflash.
    UnsupportedHardware,
    /// A different device than the one previously identified is attached.
    DeviceMismatch,
    /// An input file is missing, unreadable, or has an invalid format.
    InvalidFile,
    /// The firmware does not match the target chip or its revision.
    IncompatibleFirmware,
    /// Writing or erasing flash failed on the device.
    FlashOperation,
    /// Data written to flash did not match after verification.
    Verification,
    /// A request parameter was rejected before touching any hardware.
    InvalidInput,
    /// The operation was cancelled by the user.
    Cancelled,
    /// A local file system operation failed.
    Io,
    /// The feature is not available for this device or configuration.
    Unsupported,
    /// An unexpected internal failure.
    Internal,
}

/// Application error with a user-facing message and optional technical
/// detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct Error {
    pub kind: ErrorKind,
    /// Concise, human-readable description of what went wrong.
    pub message: String,
    /// Suggested next step for the user, if one is known.
    pub hint: Option<String>,
    /// Underlying technical diagnostic.
    pub detail: Option<String>,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
            detail: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn invalid_file(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidFile, message)
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "The operation was cancelled.")
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }

    /// Map an I/O error that occurred while accessing `path`.
    pub fn file_io(err: &io::Error, path: &Path, action: &str) -> Self {
        let name = display_name(path);
        let (kind, message, hint) = match err.kind() {
            io::ErrorKind::NotFound => (
                ErrorKind::InvalidFile,
                format!("Could not {action} \"{name}\": the file does not exist."),
                None,
            ),
            io::ErrorKind::PermissionDenied => (
                ErrorKind::Permission,
                format!("Could not {action} \"{name}\": permission denied."),
                Some("Choose a location you have access to."),
            ),
            io::ErrorKind::StorageFull => (
                ErrorKind::Io,
                format!("Could not {action} \"{name}\": the disk is full."),
                None,
            ),
            _ => (ErrorKind::Io, format!("Could not {action} \"{name}\"."), None),
        };
        let mut error = Self::new(kind, message).with_detail(err.to_string());
        error.hint = hint.map(str::to_owned);
        error
    }

    /// Map an error from the serial port layer that occurred while opening or
    /// using `port`.
    pub fn serial(err: &serialport::Error, port: &str) -> Self {
        let detail = err.to_string();
        match err.kind() {
            serialport::ErrorKind::NoDevice => {
                Self::new(ErrorKind::Disconnected, format!("Serial port {port} is no longer available."))
                    .with_hint("Check the USB cable and refresh the port list.")
                    .with_detail(detail)
            }
            serialport::ErrorKind::Io(kind) => io_kind_to_serial_error(kind, port, detail),
            _ => {
                if looks_busy(&detail) {
                    busy_port_error(port, detail)
                } else {
                    Self::new(ErrorKind::Connection, format!("Could not open serial port {port}."))
                        .with_detail(detail)
                }
            }
        }
    }
}

fn io_kind_to_serial_error(kind: io::ErrorKind, port: &str, detail: String) -> Error {
    match kind {
        io::ErrorKind::NotFound => {
            Error::new(ErrorKind::Disconnected, format!("Serial port {port} was not found."))
                .with_hint("The device may have been unplugged. Refresh the port list.")
                .with_detail(detail)
        }
        io::ErrorKind::PermissionDenied => permission_error(port, detail),
        io::ErrorKind::ResourceBusy => busy_port_error(port, detail),
        io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected | io::ErrorKind::UnexpectedEof => {
            Error::new(ErrorKind::Disconnected, format!("The connection to {port} was lost."))
                .with_hint("Check the USB cable and reconnect the device.")
                .with_detail(detail)
        }
        _ if looks_busy(&detail) => busy_port_error(port, detail),
        _ if looks_permission(&detail) => permission_error(port, detail),
        _ => Error::new(ErrorKind::Connection, format!("Communication with {port} failed."))
            .with_detail(detail),
    }
}

fn busy_port_error(port: &str, detail: String) -> Error {
    Error::new(ErrorKind::PortBusy, format!("Serial port {port} is in use by another application."))
        .with_hint("Close other serial monitors, IDEs or terminal sessions using this port.")
        .with_detail(detail)
}

fn permission_error(port: &str, detail: String) -> Error {
    let hint = if cfg!(target_os = "linux") {
        "Add your user to the group that owns the device (often 'dialout' or 'uucp')."
    } else {
        "Make sure no other application holds the port and that the driver is installed."
    };
    Error::new(ErrorKind::Permission, format!("Access to serial port {port} was denied."))
        .with_hint(hint)
        .with_detail(detail)
}

fn looks_busy(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("busy") || lower.contains("in use") || lower.contains("exclusive")
}

fn looks_permission(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("permission") || lower.contains("access is denied")
}

/// File name used in user-facing messages. Full paths are avoided so that
/// messages and logs do not expose local directory structures unnecessarily.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Flatten an error and its sources into a single diagnostic string.
pub fn error_chain(err: &dyn StdError) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        let inner_text = inner.to_string();
        if !text.contains(&inner_text) {
            text.push_str(": ");
            text.push_str(&inner_text);
        }
        source = inner.source();
    }
    text
}

fn find_source<'a, T: StdError + 'static>(err: &'a (dyn StdError + 'static)) -> Option<&'a T> {
    let mut current: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(e) = current {
        if let Some(found) = e.downcast_ref::<T>() {
            return Some(found);
        }
        current = e.source();
    }
    None
}

/// Translate an espflash error into an application error.
///
/// `port` is used to produce precise messages for serial failures.
pub fn from_espflash(err: espflash::Error, port: &str) -> Error {
    use espflash::Error as E;

    let detail = error_chain(&err);

    if let Some(serial) = find_source::<serialport::Error>(&err) {
        return Error::serial(serial, port).with_detail(detail);
    }

    match &err {
        E::Cancelled => Error::cancelled(),
        E::ChipDetectError(_) => Error::new(
            ErrorKind::UnsupportedHardware,
            "The connected chip could not be identified as a supported Espressif device.",
        )
        .with_hint("OxidizeESP supports the ESP32 family handled by espflash. ESP8266 is not supported.")
        .with_detail(detail),
        E::ChipMismatch(expected, found) => Error::new(
            ErrorKind::DeviceMismatch,
            format!("Expected an {expected}, but an {found} is connected to {port}."),
        )
        .with_hint("Reconnect the device to identify it again.")
        .with_detail(detail),
        E::FirmwareChipMismatch { elf, detected } => Error::new(
            ErrorKind::IncompatibleFirmware,
            format!("The firmware was built for {elf}, but the connected chip is {detected}."),
        )
        .with_detail(detail),
        E::UnsupportedChipRevision {
            major,
            minor,
            found_major,
            found_minor,
        } => Error::new(
            ErrorKind::IncompatibleFirmware,
            format!(
                "The firmware requires chip revision v{major}.{minor} or newer, but the device is v{found_major}.{found_minor}."
            ),
        )
        .with_detail(detail),
        E::VerifyFailed | E::DigestMismatch(..) => Error::new(
            ErrorKind::Verification,
            "Flash verification failed: the data on the device does not match.",
        )
        .with_hint("Retry at a lower baud rate. The flash contents may be inconsistent.")
        .with_detail(detail),
        E::Flashing(_) => Error::new(ErrorKind::FlashOperation, "Communication failed while writing flash.")
            .with_hint("The flash may contain partially written data. Retry the operation, optionally at a lower baud rate.")
            .with_detail(detail),
        E::RomError(_) => Error::new(ErrorKind::FlashOperation, "The device bootloader rejected a command.")
            .with_detail(detail),
        E::FlashConnect => Error::new(
            ErrorKind::Connection,
            "The on-chip flash memory did not respond.",
        )
        .with_hint("The module may use a non-standard flash configuration or be damaged.")
        .with_detail(detail),
        E::UnsupportedFlash(_) => Error::new(ErrorKind::UnsupportedHardware, "The attached flash chip is not supported.")
            .with_detail(detail),
        E::StubRequired => Error::unsupported("This operation requires the RAM flasher stub.")
            .with_hint("Enable \"Use flasher stub\" in the connection options.")
            .with_detail(detail),
        E::UnsupportedFeature { chip, feature } => {
            Error::unsupported(format!("The {chip} does not support {feature}.")).with_detail(detail)
        }
        E::UnsupportedFlashFrequency { chip, frequency } => Error::invalid_input(format!(
            "The flash frequency {frequency} is not supported by the {chip}."
        ))
        .with_detail(detail),
        E::InvalidElf(_) | E::ElfNotRamLoadable | E::ElfTooBig(..) => {
            Error::invalid_file("The selected file is not a valid firmware ELF for this chip.").with_detail(detail)
        }
        E::InvalidAppDescriptor(_) | E::AppDescriptorNotPresent(_) => Error::invalid_file(
            "The ELF file does not contain a valid ESP-IDF application descriptor.",
        )
        .with_hint("esp-hal applications need the esp_app_desc!() macro from esp-bootloader-esp-idf.")
        .with_detail(detail),
        E::InvalidBootloader | E::InvalidBootloaderPath => {
            Error::invalid_file("The bootloader image is invalid.").with_detail(detail)
        }
        E::Partition(_) | E::InvalidPartitionTablePath | E::MissingPartitionTable(_) | E::MissingPartition(_) => {
            Error::invalid_file("The partition table is invalid.").with_detail(detail)
        }
        E::PartitionTableDoesNotFit(size) => Error::new(
            ErrorKind::IncompatibleFirmware,
            format!("The partition table does not fit into a flash of {size}."),
        )
        .with_detail(detail),
        E::AppPartitionNotFound => {
            Error::invalid_file("No suitable application partition was found in the partition table.")
                .with_detail(detail)
        }
        E::SecureDownloadBootloaderProtection => Error::new(
            ErrorKind::Unsupported,
            "Writing below 0x8000 is blocked because Secure Download Mode is enabled.",
        )
        .with_detail(detail),
        E::FileOpenError(..) => Error::new(ErrorKind::InvalidFile, "A required file could not be opened.").with_detail(detail),
        E::Connection(_) => connection_error(&detail, port),
        _ => connection_or_internal(&detail, port),
    }
}

fn connection_error(detail: &str, port: &str) -> Error {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("serial port not found") {
        return Error::new(ErrorKind::Disconnected, format!("Serial port {port} is no longer available."))
            .with_hint("The device may have been unplugged. Refresh the port list.")
            .with_detail(detail);
    }
    if lower.contains("wrong boot mode") {
        return Error::new(ErrorKind::Connection, "The chip is not in download mode.")
            .with_hint("Hold the BOOT button while pressing RESET, then try again.")
            .with_detail(detail);
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return Error::new(ErrorKind::Connection, format!("The device on {port} stopped responding."))
            .with_hint("Check the cable, try a lower baud rate, or put the chip into download mode manually.")
            .with_detail(detail);
    }
    Error::new(ErrorKind::Connection, format!("Could not connect to an Espressif bootloader on {port}."))
        .with_hint("Make sure an ESP32-family device is attached. If the board has no auto-reset circuit, hold BOOT while pressing RESET.")
        .with_detail(detail)
}

fn connection_or_internal(detail: &str, port: &str) -> Error {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("serial") || lower.contains("timeout") || lower.contains("slip") || lower.contains("response") {
        connection_error(detail, port)
    } else {
        Error::internal("The operation failed unexpectedly.").with_detail(detail)
    }
}

impl From<esp_idf_part::Error> for Error {
    fn from(err: esp_idf_part::Error) -> Self {
        use esp_idf_part::Error as P;
        let message = match &err {
            P::OverlappingPartitions(a, b) => format!("Partitions \"{a}\" and \"{b}\" overlap."),
            P::DuplicatePartitions(name) => format!("The partition name \"{name}\" is used more than once."),
            P::UnalignedPartition => {
                "A partition is not correctly aligned (app partitions require 64 KiB, data partitions 4 KiB).".to_owned()
            }
            P::NoAppPartition => "The partition table does not contain an app partition.".to_owned(),
            P::MultipleFactoryPartitions => "The partition table contains more than one factory app partition.".to_owned(),
            P::MultipleOtadataPartitions => "The partition table contains more than one otadata partition.".to_owned(),
            P::InvalidOtadataPartitionSize => "The otadata partition must be exactly 8 KiB.".to_owned(),
            P::PartitionTooLarge(name) => format!("App partition \"{name}\" exceeds the 16 MiB limit."),
            P::InvalidChecksum { .. } => "The partition table checksum is invalid.".to_owned(),
            P::NoEndMarker => "The binary partition table has no end marker.".to_owned(),
            P::LengthNotMultipleOf32 => "A binary partition table must be a multiple of 32 bytes long.".to_owned(),
            _ => "The partition table could not be parsed.".to_owned(),
        };
        Error::invalid_file(message).with_detail(error_chain(&err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_missing_file() {
        let err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let mapped = Error::file_io(&err, Path::new("/some/dir/app.elf"), "read");
        assert_eq!(mapped.kind, ErrorKind::InvalidFile);
        assert!(mapped.message.contains("app.elf"));
        assert!(!mapped.message.contains("/some/dir"));
    }

    #[test]
    fn maps_permission_denied_file() {
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let mapped = Error::file_io(&err, Path::new("out.bin"), "write");
        assert_eq!(mapped.kind, ErrorKind::Permission);
        assert!(mapped.hint.is_some());
    }

    #[test]
    fn maps_serial_errors() {
        let gone = serialport::Error::new(serialport::ErrorKind::NoDevice, "gone");
        assert_eq!(Error::serial(&gone, "COM3").kind, ErrorKind::Disconnected);

        let busy = serialport::Error::new(serialport::ErrorKind::Io(io::ErrorKind::Other), "Resource busy");
        assert_eq!(Error::serial(&busy, "/dev/cu.usbserial").kind, ErrorKind::PortBusy);

        let denied = serialport::Error::new(
            serialport::ErrorKind::Io(io::ErrorKind::PermissionDenied),
            "Permission denied",
        );
        assert_eq!(Error::serial(&denied, "/dev/ttyUSB0").kind, ErrorKind::Permission);
    }

    #[test]
    fn maps_espflash_errors() {
        let err = from_espflash(espflash::Error::VerifyFailed, "COM4");
        assert_eq!(err.kind, ErrorKind::Verification);

        let err = from_espflash(espflash::Error::ChipMismatch("esp32".into(), "esp32c3".into()), "COM4");
        assert_eq!(err.kind, ErrorKind::DeviceMismatch);

        let err = from_espflash(espflash::Error::StubRequired, "COM4");
        assert_eq!(err.kind, ErrorKind::Unsupported);

        let err = from_espflash(espflash::Error::ChipDetectError("magic".into()), "COM4");
        assert_eq!(err.kind, ErrorKind::UnsupportedHardware);
    }

    #[test]
    fn maps_espflash_io_serial_error_through_source_chain() {
        let serial = serialport::Error::new(serialport::ErrorKind::NoDevice, "device removed");
        let err = from_espflash(espflash::Error::from(serial), "/dev/cu.usbmodem1");
        assert_eq!(err.kind, ErrorKind::Disconnected);
    }

    #[test]
    fn maps_partition_errors() {
        let err: Error = esp_idf_part::Error::OverlappingPartitions("a".into(), "b".into()).into();
        assert_eq!(err.kind, ErrorKind::InvalidFile);
        assert!(err.message.contains("overlap"));
    }
}
