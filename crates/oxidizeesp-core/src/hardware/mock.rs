//! Simulated hardware for development and automated tests.
//!
//! Only compiled with the `mock` feature or in tests. Simulated ports are
//! clearly named (`SIMULATED-*`) so they cannot be mistaken for hardware.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use espflash::{image_format::Segment, target::ProgressCallbacks};
use md5::{Digest, Md5};

use super::{Backend, ChipInfo, FlashLink, SecurityState, SerialLink, WriteBehavior};
use crate::{
    Error, ErrorKind, Result,
    model::{Chip, ConnectionOptions, FlashSize, XtalFreq},
    ports::{PortHint, PortKind, SerialPortDescriptor, UsbInfo},
    region::{FlashRegion, SECTOR_SIZE},
};

/// State of one simulated device.
#[derive(Debug)]
pub struct SimDevice {
    pub port: SerialPortDescriptor,
    pub chip: Chip,
    pub mac: String,
    pub flash: Vec<u8>,
    pub plugged_in: bool,
    /// Fail the next connection attempt with this error.
    pub fail_connect: Option<Error>,
    /// Number of `connect` calls, for assertions.
    pub connect_count: usize,
    pub resets: usize,
    pub sent: Vec<u8>,
}

impl SimDevice {
    pub fn new(port_name: &str, chip: Chip, flash_size: FlashSize, serial: &str) -> Self {
        let mac_tail = serial.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(u32::from(b)));
        Self {
            port: SerialPortDescriptor {
                name: port_name.to_owned(),
                kind: PortKind::Usb,
                usb: Some(UsbInfo {
                    vid: 0x10C4,
                    pid: 0xEA60,
                    serial_number: Some(serial.to_owned()),
                    manufacturer: Some("Simulated".to_owned()),
                    product: Some("Simulated ESP device".to_owned()),
                }),
                hint: PortHint::UsbUartBridge,
                bridge: Some("Simulated bridge".to_owned()),
            },
            chip,
            mac: format!(
                "02:00:{:02x}:{:02x}:{:02x}:{:02x}",
                (mac_tail >> 24) as u8,
                (mac_tail >> 16) as u8,
                (mac_tail >> 8) as u8,
                mac_tail as u8
            ),
            flash: vec![0xFF; flash_size.bytes() as usize],
            plugged_in: true,
            fail_connect: None,
            connect_count: 0,
            resets: 0,
            sent: Vec::new(),
        }
    }
}

pub type SharedDevice = Arc<Mutex<SimDevice>>;

fn lock(device: &SharedDevice) -> MutexGuard<'_, SimDevice> {
    device.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Backend with a configurable set of simulated devices.
#[derive(Debug, Default, Clone)]
pub struct MockBackend {
    devices: Arc<Mutex<Vec<SharedDevice>>>,
    /// Artificial delay per written or read block to make progress visible
    /// during development. Zero in tests.
    pub block_delay: Duration,
}

impl MockBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Backend preloaded with two devices for interactive development.
    pub fn with_demo_devices() -> Self {
        let backend = Self { block_delay: Duration::from_millis(15), ..Self::default() };
        backend.add(SimDevice::new("SIMULATED-ESP32-C3", Chip::Esp32c3, FlashSize::Mb4, "SIM-0001"));
        backend.add(SimDevice::new("SIMULATED-ESP32-S3", Chip::Esp32s3, FlashSize::Mb8, "SIM-0002"));
        backend
    }

    pub fn add(&self, device: SimDevice) -> SharedDevice {
        let shared = Arc::new(Mutex::new(device));
        self.devices.lock().unwrap_or_else(|p| p.into_inner()).push(shared.clone());
        shared
    }

    fn find(&self, port: &str) -> Result<SharedDevice> {
        self.devices
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|d| {
                let d = lock(d);
                d.port.name == port && d.plugged_in
            })
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::Disconnected, format!("Serial port {port} was not found.")))
    }
}

impl Backend for MockBackend {
    fn is_simulated(&self) -> bool {
        true
    }

    fn list_ports(&self) -> Result<Vec<SerialPortDescriptor>> {
        Ok(self
            .devices
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|d| lock(d))
            .filter(|d| d.plugged_in)
            .map(|d| d.port.clone())
            .collect())
    }

    fn connect(
        &self,
        port: &SerialPortDescriptor,
        options: &ConnectionOptions,
        behavior: WriteBehavior,
        expected_chip: Option<Chip>,
    ) -> Result<Box<dyn FlashLink>> {
        options.validate()?;
        let device = self.find(&port.name)?;
        let chip = {
            let mut d = lock(&device);
            d.connect_count += 1;
            if let Some(err) = d.fail_connect.take() {
                return Err(err);
            }
            d.chip
        };
        if let Some(expected) = expected_chip
            && expected != chip
        {
            return Err(Error::new(
                ErrorKind::DeviceMismatch,
                format!("Expected an {expected:?}, but an {chip:?} is connected."),
            ));
        }
        Ok(Box::new(MockLink {
            device,
            behavior,
            use_stub: options.use_stub,
            flash_size_override: None,
            block_delay: self.block_delay,
        }))
    }

    fn open_serial(&self, port: &SerialPortDescriptor, _baud: u32) -> Result<Box<dyn SerialLink>> {
        let device = self.find(&port.name)?;
        Ok(Box::new(MockSerial {
            device,
            started: Instant::now(),
            next_line: 0,
            pending: Vec::new(),
            timeout: Duration::from_millis(20),
        }))
    }
}

struct MockLink {
    device: SharedDevice,
    behavior: WriteBehavior,
    use_stub: bool,
    flash_size_override: Option<FlashSize>,
    block_delay: Duration,
}

impl MockLink {
    fn ensure_connected(&self) -> Result<MutexGuard<'_, SimDevice>> {
        let d = lock(&self.device);
        if d.plugged_in {
            Ok(d)
        } else {
            Err(Error::new(ErrorKind::Disconnected, format!("The connection to {} was lost.", d.port.name)))
        }
    }

    fn check_region(flash: &[u8], offset: u32, length: u32) -> Result<std::ops::Range<usize>> {
        let region = FlashRegion::new(offset, length);
        region.validate(crate::region::Alignment::None, Some(flash.len() as u32))?;
        Ok(offset as usize..(offset + length) as usize)
    }

    fn pause(&self) {
        if !self.block_delay.is_zero() {
            thread::sleep(self.block_delay);
        }
    }
}

impl FlashLink for MockLink {
    fn chip(&self) -> Chip {
        lock(&self.device).chip
    }

    fn chip_info(&mut self) -> Result<ChipInfo> {
        let d = self.ensure_connected()?;
        let size = self
            .flash_size_override
            .or_else(|| FlashSize::ALL.into_iter().find(|s| s.bytes() as usize == d.flash.len()));
        Ok(ChipInfo {
            chip: d.chip,
            chip_name: d.chip.display_name().to_owned(),
            revision: Some("0.4".to_owned()),
            revision_number: Some(4),
            crystal: XtalFreq::Mhz40,
            flash_size: size,
            features: vec!["Simulated device".to_owned()],
            mac_address: Some(d.mac.clone()),
            security: Some(SecurityState {
                secure_boot: false,
                flash_encryption: false,
                secure_download_mode: false,
                jtag_disabled: false,
            }),
            stub_loaded: self.use_stub,
        })
    }

    fn set_flash_size(&mut self, size: FlashSize) {
        self.flash_size_override = Some(size);
    }

    fn write_segments(&mut self, segments: &[Segment<'_>], progress: &mut dyn ProgressCallbacks) -> Result<()> {
        const BLOCK: usize = 0x4000;
        for segment in segments {
            let range = {
                let d = self.ensure_connected()?;
                Self::check_region(&d.flash, segment.addr, segment.size())?
            };
            let data = segment.data();
            if self.behavior.skip_unchanged && self.ensure_connected()?.flash[range.clone()] == *data {
                progress.finish(true);
                continue;
            }
            let blocks = data.len().div_ceil(BLOCK).max(1);
            progress.init(segment.addr, blocks);
            for (i, chunk) in data.chunks(BLOCK).enumerate() {
                self.pause();
                let mut d = self.ensure_connected()?;
                let start = range.start + i * BLOCK;
                d.flash[start..start + chunk.len()].copy_from_slice(chunk);
                drop(d);
                progress.update(i + 1);
            }
            if self.behavior.verify {
                progress.verifying();
                let d = self.ensure_connected()?;
                if Md5::digest(&d.flash[range.clone()]) != Md5::digest(data) {
                    return Err(Error::new(ErrorKind::Verification, "Flash verification failed."));
                }
            }
            progress.finish(false);
        }
        self.reset_after()
    }

    fn read_flash(&mut self, offset: u32, length: u32, dest: &Path) -> Result<()> {
        self.pause();
        let data = {
            let d = self.ensure_connected()?;
            let range = Self::check_region(&d.flash, offset, length)?;
            d.flash[range].to_vec()
        };
        std::fs::write(dest, data).map_err(|e| Error::file_io(&e, dest, "write"))
    }

    fn erase_region(&mut self, offset: u32, length: u32) -> Result<()> {
        if !self.use_stub {
            return Err(Error::unsupported("This operation requires the RAM flasher stub."));
        }
        if !offset.is_multiple_of(SECTOR_SIZE) || !length.is_multiple_of(SECTOR_SIZE) {
            return Err(Error::invalid_input("Erase regions must be sector aligned."));
        }
        self.pause();
        let mut d = self.ensure_connected()?;
        let range = Self::check_region(&d.flash, offset, length)?;
        d.flash[range].fill(0xFF);
        Ok(())
    }

    fn erase_all(&mut self) -> Result<()> {
        if !self.use_stub {
            return Err(Error::unsupported("This operation requires the RAM flasher stub."));
        }
        self.pause();
        self.ensure_connected()?.flash.fill(0xFF);
        Ok(())
    }

    fn checksum_md5(&mut self, offset: u32, length: u32) -> Result<u128> {
        let d = self.ensure_connected()?;
        let range = Self::check_region(&d.flash, offset, length)?;
        let digest = Md5::digest(&d.flash[range]);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest);
        Ok(u128::from_be_bytes(bytes))
    }

    fn reset_after(&mut self) -> Result<()> {
        self.ensure_connected()?.resets += 1;
        Ok(())
    }
}

struct MockSerial {
    device: SharedDevice,
    started: Instant,
    next_line: u64,
    pending: Vec<u8>,
    timeout: Duration,
}

impl SerialLink for MockSerial {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        {
            let d = lock(&self.device);
            if !d.plugged_in {
                return Err(Error::new(
                    ErrorKind::Disconnected,
                    format!("The connection to {} was lost.", d.port.name),
                ));
            }
        }
        if self.pending.is_empty() {
            let due = self.started.elapsed().as_millis() as u64 / 250;
            if self.next_line > due {
                thread::sleep(self.timeout);
                return Ok(0);
            }
            let n = self.next_line;
            self.next_line += 1;
            let line = match n {
                0 => "ESP-ROM:esp32c3-api1-20210207\r\n".to_owned(),
                1 => "rst:0x1 (POWERON),boot:0xc (SPI_FAST_FLASH_BOOT)\r\n".to_owned(),
                _ if n.is_multiple_of(20) => {
                    format!("\x1b[0;33mW ({}) app: simulated warning, PC=0x42000a10\x1b[0m\r\n", n * 250)
                }
                _ => format!("\x1b[0;32mI ({}) app: simulated tick {n}\x1b[0m\r\n", n * 250),
            };
            self.pending.extend_from_slice(line.as_bytes());
        }
        let count = buf.len().min(self.pending.len());
        buf[..count].copy_from_slice(&self.pending[..count]);
        self.pending.drain(..count);
        Ok(count)
    }

    fn write_all(&mut self, data: &[u8]) -> Result<()> {
        lock(&self.device).sent.extend_from_slice(data);
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        lock(&self.device).resets += 1;
        self.started = Instant::now();
        self.next_line = 0;
        self.pending.clear();
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.timeout = timeout;
        Ok(())
    }
}
