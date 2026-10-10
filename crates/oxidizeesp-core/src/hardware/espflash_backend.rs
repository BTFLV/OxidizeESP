//! Hardware backend built on the espflash library.

use std::{io, path::Path, time::Duration};

use espflash::{
    connection::{Connection, Port, ResetAfterOperation, ResetBeforeOperation},
    flasher::Flasher,
    image_format::Segment,
    target::ProgressCallbacks,
};
use serialport::{FlowControl, SerialPort, UsbPortInfo};

use super::{Backend, ChipInfo, FlashLink, SecurityState, SerialLink, WriteBehavior};
use crate::{
    Error, ErrorKind, Result,
    error::from_espflash,
    model::{Chip, ConnectionOptions, FlashSize, ROM_BAUD, XtalFreq},
    ports::{self, SerialPortDescriptor},
    region::SECTOR_SIZE,
};

/// Bytes per read request when the flasher stub is used (espflash default).
const READ_BLOCK_SIZE: u32 = SECTOR_SIZE;
/// Maximum number of unacknowledged read packets (espflash default).
const READ_MAX_IN_FLIGHT: u32 = 64;

/// Production backend talking to real devices.
#[derive(Debug, Default)]
pub struct EspflashBackend;

impl EspflashBackend {
    pub fn new() -> Self {
        Self
    }
}

fn usb_port_info(port: &SerialPortDescriptor) -> UsbPortInfo {
    match &port.usb {
        Some(usb) => UsbPortInfo {
            vid: usb.vid,
            pid: usb.pid,
            serial_number: usb.serial_number.clone(),
            manufacturer: usb.manufacturer.clone(),
            product: usb.product.clone(),
        },
        None => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
    }
}

fn open_port(name: &str, baud: u32) -> Result<Port> {
    serialport::new(name, baud)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(50))
        .open_native()
        .map_err(|e| Error::serial(&e, name))
}

impl Backend for EspflashBackend {
    fn is_simulated(&self) -> bool {
        false
    }

    fn list_ports(&self) -> Result<Vec<SerialPortDescriptor>> {
        ports::list_system_ports()
    }

    fn connect(
        &self,
        port: &SerialPortDescriptor,
        options: &ConnectionOptions,
        behavior: WriteBehavior,
        expected_chip: Option<Chip>,
    ) -> Result<Box<dyn FlashLink>> {
        options.validate()?;
        let serial = open_port(&port.name, ROM_BAUD)?;
        let connection = Connection::new(
            serial,
            usb_port_info(port),
            options.reset_after.to_espflash(),
            options.reset_before.to_espflash(),
            options.baud,
        );
        let flasher = Flasher::connect(
            connection,
            options.use_stub,
            behavior.verify,
            behavior.skip_unchanged,
            expected_chip.map(Chip::to_espflash),
            Some(options.baud),
        )
        .map_err(|e| from_espflash(e, &port.name))?;

        let chip = Chip::from_espflash(flasher.chip()).ok_or_else(|| {
            Error::new(ErrorKind::UnsupportedHardware, "The detected chip is not supported by this version of OxidizeESP.")
        })?;

        Ok(Box::new(EspflashLink {
            flasher,
            chip,
            port: port.name.clone(),
            use_stub: options.use_stub,
        }))
    }

    fn open_serial(&self, port: &SerialPortDescriptor, baud: u32) -> Result<Box<dyn SerialLink>> {
        let mut serial = open_port(&port.name, baud)?;
        // Release both control lines so an auto-reset circuit does not hold
        // the chip in reset or in download mode while monitoring.
        let _ = serial.write_data_terminal_ready(false);
        let _ = serial.write_request_to_send(false);
        Ok(Box::new(NativeSerial {
            port: serial,
            usb: usb_port_info(port),
            name: port.name.clone(),
            baud,
        }))
    }
}

struct EspflashLink {
    flasher: Flasher,
    chip: Chip,
    port: String,
    use_stub: bool,
}

impl EspflashLink {
    fn map(&self, err: espflash::Error) -> Error {
        from_espflash(err, &self.port)
    }
}

fn security_state(info: &espflash::connection::SecurityInfo) -> SecurityState {
    // Bit positions as defined by the ROM GET_SECURITY_INFO command.
    const SECURE_BOOT_EN: u32 = 1 << 0;
    const SECURE_DOWNLOAD_ENABLE: u32 = 1 << 2;
    const SOFT_DIS_JTAG: u32 = 1 << 6;
    const HARD_DIS_JTAG: u32 = 1 << 7;
    SecurityState {
        secure_boot: info.flags & SECURE_BOOT_EN != 0,
        // Flash encryption is enabled when an odd number of bits is set in
        // SPI_BOOT_CRYPT_CNT / FLASH_CRYPT_CNT.
        flash_encryption: info.flash_crypt_cnt.count_ones() % 2 == 1,
        secure_download_mode: info.flags & SECURE_DOWNLOAD_ENABLE != 0,
        jtag_disabled: info.flags & (SOFT_DIS_JTAG | HARD_DIS_JTAG) != 0,
    }
}

impl FlashLink for EspflashLink {
    fn chip(&self) -> Chip {
        self.chip
    }

    fn chip_info(&mut self) -> Result<ChipInfo> {
        let sdm = self.flasher.secure_download_mode();
        let info = self.flasher.device_info().map_err(|e| self.map(e))?;
        let security = self
            .flasher
            .security_info()
            .ok()
            .map(|s| security_state(&s))
            .or_else(|| {
                sdm.then_some(SecurityState {
                    secure_boot: false,
                    flash_encryption: false,
                    secure_download_mode: true,
                    jtag_disabled: false,
                })
            });
        let revision_number = info.revision.and_then(|(major, minor)| u16::try_from(major * 100 + minor).ok());
        Ok(ChipInfo {
            chip: self.chip,
            chip_name: self.chip.display_name().to_owned(),
            revision: info.revision.map(|(major, minor)| format!("{major}.{minor}")),
            revision_number,
            crystal: XtalFreq::from_espflash(info.crystal_frequency),
            // Flash size detection is not possible in Secure Download Mode.
            flash_size: if sdm { None } else { FlashSize::from_espflash(info.flash_size) },
            features: info.features,
            mac_address: info.mac_address,
            security,
            stub_loaded: self.use_stub && !sdm,
        })
    }

    fn set_flash_size(&mut self, size: FlashSize) {
        self.flasher.set_flash_size(size.to_espflash());
    }

    fn write_segments(&mut self, segments: &[Segment<'_>], progress: &mut dyn ProgressCallbacks) -> Result<()> {
        self.flasher
            .write_bins_to_flash(segments, progress)
            .map_err(|e| self.map(e))
    }

    fn read_flash(&mut self, offset: u32, length: u32, dest: &Path) -> Result<()> {
        let result = if self.use_stub {
            self.flasher
                .read_flash(offset, length, READ_BLOCK_SIZE, READ_MAX_IN_FLIGHT, dest.to_path_buf())
        } else {
            self.flasher
                .read_flash_rom(offset, length, READ_BLOCK_SIZE, READ_MAX_IN_FLIGHT, dest.to_path_buf())
        };
        result.map_err(|e| self.map(e))
    }

    fn erase_region(&mut self, offset: u32, length: u32) -> Result<()> {
        if !self.use_stub {
            return Err(from_espflash(espflash::Error::StubRequired, &self.port));
        }
        self.flasher.erase_region(offset, length).map_err(|e| self.map(e))
    }

    fn erase_all(&mut self) -> Result<()> {
        if !self.use_stub {
            return Err(from_espflash(espflash::Error::StubRequired, &self.port));
        }
        self.flasher.erase_flash().map_err(|e| self.map(e))
    }

    fn checksum_md5(&mut self, offset: u32, length: u32) -> Result<u128> {
        self.flasher.checksum_md5(offset, length).map_err(|e| self.map(e))
    }

    fn reset_after(&mut self) -> Result<()> {
        let chip = self.flasher.chip();
        let use_stub = self.use_stub;
        self.flasher
            .connection()
            .reset_after(use_stub, chip)
            .map_err(|e| from_espflash(e, &self.port))
    }
}

struct NativeSerial {
    port: Port,
    usb: UsbPortInfo,
    name: String,
    baud: u32,
}

/// Map an I/O error from an open serial port. Unplugging a device surfaces
/// as different error codes depending on the platform, so any persistent
/// error other than a timeout is treated as a lost connection.
fn serial_io_error(err: &io::Error, port: &str) -> Error {
    match err.kind() {
        io::ErrorKind::PermissionDenied => {
            Error::new(ErrorKind::Permission, format!("Access to serial port {port} was denied."))
                .with_detail(err.to_string())
        }
        _ => Error::new(ErrorKind::Disconnected, format!("The connection to {port} was lost."))
            .with_hint("Check the USB cable. The device may have been unplugged or reset into a different USB mode.")
            .with_detail(err.to_string()),
    }
}

impl SerialLink for NativeSerial {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match io::Read::read(&mut self.port, buf) {
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => Ok(0),
            Err(e) => Err(serial_io_error(&e, &self.name)),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<()> {
        io::Write::write_all(&mut self.port, data)
            .and_then(|()| io::Write::flush(&mut self.port))
            .map_err(|e| serial_io_error(&e, &self.name))
    }

    fn reset(&mut self) -> Result<()> {
        let clone = self.port.try_clone_native().map_err(|e| Error::serial(&e, &self.name))?;
        let mut connection = Connection::new(
            clone,
            self.usb.clone(),
            ResetAfterOperation::HardReset,
            ResetBeforeOperation::DefaultReset,
            self.baud,
        );
        connection.reset().map_err(|e| from_espflash(e, &self.name))
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.port.set_timeout(timeout).map_err(|e| Error::serial(&e, &self.name))
    }
}
