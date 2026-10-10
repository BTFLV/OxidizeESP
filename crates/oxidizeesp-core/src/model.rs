//! Typed data model exchanged between the frontend and the Rust core.
//!
//! espflash types are mirrored here so that the public interface of the
//! application is stable, serializable and exported to TypeScript. Conversion
//! functions translate to the espflash equivalents at the hardware boundary.

use espflash::{
    connection::{ResetAfterOperation, ResetBeforeOperation},
    flasher::{FlashFrequency as EfFreq, FlashMode as EfMode, FlashSize as EfSize},
    target::{Chip as EfChip, XtalFrequency},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Espressif chips supported by espflash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Chip {
    Esp32,
    Esp32c2,
    Esp32c3,
    Esp32c5,
    Esp32c6,
    Esp32c61,
    Esp32h2,
    Esp32h4,
    Esp32p4,
    Esp32s2,
    Esp32s3,
    Esp32s31,
}

impl Chip {
    pub const ALL: [Chip; 12] = [
        Chip::Esp32,
        Chip::Esp32c2,
        Chip::Esp32c3,
        Chip::Esp32c5,
        Chip::Esp32c6,
        Chip::Esp32c61,
        Chip::Esp32h2,
        Chip::Esp32h4,
        Chip::Esp32p4,
        Chip::Esp32s2,
        Chip::Esp32s3,
        Chip::Esp32s31,
    ];

    pub fn to_espflash(self) -> EfChip {
        match self {
            Chip::Esp32 => EfChip::Esp32,
            Chip::Esp32c2 => EfChip::Esp32c2,
            Chip::Esp32c3 => EfChip::Esp32c3,
            Chip::Esp32c5 => EfChip::Esp32c5,
            Chip::Esp32c6 => EfChip::Esp32c6,
            Chip::Esp32c61 => EfChip::Esp32c61,
            Chip::Esp32h2 => EfChip::Esp32h2,
            Chip::Esp32h4 => EfChip::Esp32h4,
            Chip::Esp32p4 => EfChip::Esp32p4,
            Chip::Esp32s2 => EfChip::Esp32s2,
            Chip::Esp32s3 => EfChip::Esp32s3,
            Chip::Esp32s31 => EfChip::Esp32s31,
        }
    }

    /// Returns `None` for chips added to espflash after this release.
    pub fn from_espflash(chip: EfChip) -> Option<Self> {
        Chip::ALL.into_iter().find(|c| c.to_espflash() == chip)
    }

    /// Parse the lower-case identifier used by espflash and ESP-IDF metadata
    /// (for example `esp32c3`).
    pub fn parse(name: &str) -> Option<Self> {
        let normalized: String = name
            .chars()
            .filter(|c| *c != '-' && *c != '_')
            .collect::<String>()
            .to_ascii_lowercase();
        Chip::ALL
            .into_iter()
            .find(|c| c.to_espflash().to_string() == normalized)
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Chip::Esp32 => "ESP32",
            Chip::Esp32c2 => "ESP32-C2",
            Chip::Esp32c3 => "ESP32-C3",
            Chip::Esp32c5 => "ESP32-C5",
            Chip::Esp32c6 => "ESP32-C6",
            Chip::Esp32c61 => "ESP32-C61",
            Chip::Esp32h2 => "ESP32-H2",
            Chip::Esp32h4 => "ESP32-H4",
            Chip::Esp32p4 => "ESP32-P4",
            Chip::Esp32s2 => "ESP32-S2",
            Chip::Esp32s3 => "ESP32-S3",
            Chip::Esp32s31 => "ESP32-S31",
        }
    }

    /// Flash offset of the second-stage bootloader.
    pub fn bootloader_offset(self) -> u32 {
        self.to_espflash().boot_address()
    }
}

/// Static per-chip facts that do not require a connected device.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChipDescriptor {
    pub chip: Chip,
    pub name: String,
    pub bootloader_offset: u32,
    pub default_flash_frequency: FlashFrequency,
    pub supported_flash_frequencies: Vec<FlashFrequency>,
    pub supported_xtal_frequencies: Vec<XtalFreq>,
    pub default_xtal_frequency: XtalFreq,
    /// MMU page sizes selectable for this chip, if configurable.
    pub mmu_page_sizes: Vec<u32>,
}

impl ChipDescriptor {
    pub fn for_chip(chip: Chip) -> Self {
        let ef = chip.to_espflash();
        let encodings = ef.flash_frequency_encodings();
        let supported_flash_frequencies = FlashFrequency::ALL
            .into_iter()
            .filter(|f| encodings.contains_key(&f.to_espflash()))
            .collect();
        let supported_xtal_frequencies = match chip {
            Chip::Esp32 | Chip::Esp32c2 => vec![XtalFreq::Mhz26, XtalFreq::Mhz40],
            Chip::Esp32c5 => vec![XtalFreq::Mhz40, XtalFreq::Mhz48],
            Chip::Esp32h2 | Chip::Esp32h4 => vec![XtalFreq::Mhz32],
            _ => vec![XtalFreq::Mhz40],
        };
        Self {
            chip,
            name: chip.display_name().to_owned(),
            bootloader_offset: ef.boot_address(),
            default_flash_frequency: FlashFrequency::from_espflash(ef.default_flash_frequency())
                .unwrap_or(FlashFrequency::Mhz40),
            supported_flash_frequencies,
            supported_xtal_frequencies,
            default_xtal_frequency: XtalFreq::from_espflash(ef.default_xtal_frequency()),
            mmu_page_sizes: ef.valid_mmu_page_sizes().map(<[u32]>::to_vec).unwrap_or_default(),
        }
    }
}

/// SPI flash access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum FlashMode {
    Qio,
    Qout,
    Dio,
    Dout,
}

impl FlashMode {
    pub fn to_espflash(self) -> EfMode {
        match self {
            FlashMode::Qio => EfMode::Qio,
            FlashMode::Qout => EfMode::Qout,
            FlashMode::Dio => EfMode::Dio,
            FlashMode::Dout => EfMode::Dout,
        }
    }
}

/// SPI flash clock frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum FlashFrequency {
    #[serde(rename = "12MHz")]
    Mhz12,
    #[serde(rename = "15MHz")]
    Mhz15,
    #[serde(rename = "16MHz")]
    Mhz16,
    #[serde(rename = "20MHz")]
    Mhz20,
    #[serde(rename = "24MHz")]
    Mhz24,
    #[serde(rename = "26MHz")]
    Mhz26,
    #[serde(rename = "30MHz")]
    Mhz30,
    #[serde(rename = "40MHz")]
    Mhz40,
    #[serde(rename = "48MHz")]
    Mhz48,
    #[serde(rename = "60MHz")]
    Mhz60,
    #[serde(rename = "80MHz")]
    Mhz80,
}

impl FlashFrequency {
    pub const ALL: [FlashFrequency; 11] = [
        FlashFrequency::Mhz12,
        FlashFrequency::Mhz15,
        FlashFrequency::Mhz16,
        FlashFrequency::Mhz20,
        FlashFrequency::Mhz24,
        FlashFrequency::Mhz26,
        FlashFrequency::Mhz30,
        FlashFrequency::Mhz40,
        FlashFrequency::Mhz48,
        FlashFrequency::Mhz60,
        FlashFrequency::Mhz80,
    ];

    pub fn to_espflash(self) -> EfFreq {
        match self {
            FlashFrequency::Mhz12 => EfFreq::_12Mhz,
            FlashFrequency::Mhz15 => EfFreq::_15Mhz,
            FlashFrequency::Mhz16 => EfFreq::_16Mhz,
            FlashFrequency::Mhz20 => EfFreq::_20Mhz,
            FlashFrequency::Mhz24 => EfFreq::_24Mhz,
            FlashFrequency::Mhz26 => EfFreq::_26Mhz,
            FlashFrequency::Mhz30 => EfFreq::_30Mhz,
            FlashFrequency::Mhz40 => EfFreq::_40Mhz,
            FlashFrequency::Mhz48 => EfFreq::_48Mhz,
            FlashFrequency::Mhz60 => EfFreq::_60Mhz,
            FlashFrequency::Mhz80 => EfFreq::_80Mhz,
        }
    }

    pub fn from_espflash(freq: EfFreq) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.to_espflash() == freq)
    }
}

/// SPI flash capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum FlashSize {
    #[serde(rename = "256KB")]
    Kb256,
    #[serde(rename = "512KB")]
    Kb512,
    #[serde(rename = "1MB")]
    Mb1,
    #[serde(rename = "2MB")]
    Mb2,
    #[serde(rename = "4MB")]
    Mb4,
    #[serde(rename = "8MB")]
    Mb8,
    #[serde(rename = "16MB")]
    Mb16,
    #[serde(rename = "32MB")]
    Mb32,
    #[serde(rename = "64MB")]
    Mb64,
    #[serde(rename = "128MB")]
    Mb128,
    #[serde(rename = "256MB")]
    Mb256,
}

impl FlashSize {
    pub const ALL: [FlashSize; 11] = [
        FlashSize::Kb256,
        FlashSize::Kb512,
        FlashSize::Mb1,
        FlashSize::Mb2,
        FlashSize::Mb4,
        FlashSize::Mb8,
        FlashSize::Mb16,
        FlashSize::Mb32,
        FlashSize::Mb64,
        FlashSize::Mb128,
        FlashSize::Mb256,
    ];

    pub fn to_espflash(self) -> EfSize {
        match self {
            FlashSize::Kb256 => EfSize::_256Kb,
            FlashSize::Kb512 => EfSize::_512Kb,
            FlashSize::Mb1 => EfSize::_1Mb,
            FlashSize::Mb2 => EfSize::_2Mb,
            FlashSize::Mb4 => EfSize::_4Mb,
            FlashSize::Mb8 => EfSize::_8Mb,
            FlashSize::Mb16 => EfSize::_16Mb,
            FlashSize::Mb32 => EfSize::_32Mb,
            FlashSize::Mb64 => EfSize::_64Mb,
            FlashSize::Mb128 => EfSize::_128Mb,
            FlashSize::Mb256 => EfSize::_256Mb,
        }
    }

    pub fn from_espflash(size: EfSize) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.to_espflash() == size)
    }

    pub fn bytes(self) -> u32 {
        self.to_espflash().size()
    }
}

/// Crystal oscillator frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum XtalFreq {
    #[serde(rename = "26MHz")]
    Mhz26,
    #[serde(rename = "32MHz")]
    Mhz32,
    #[serde(rename = "40MHz")]
    Mhz40,
    #[serde(rename = "48MHz")]
    Mhz48,
}

impl XtalFreq {
    pub fn to_espflash(self) -> XtalFrequency {
        match self {
            XtalFreq::Mhz26 => XtalFrequency::_26Mhz,
            XtalFreq::Mhz32 => XtalFrequency::_32Mhz,
            XtalFreq::Mhz40 => XtalFrequency::_40Mhz,
            XtalFreq::Mhz48 => XtalFrequency::_48Mhz,
        }
    }

    pub fn from_espflash(freq: XtalFrequency) -> Self {
        match freq {
            XtalFrequency::_26Mhz => XtalFreq::Mhz26,
            XtalFrequency::_32Mhz => XtalFreq::Mhz32,
            XtalFrequency::_48Mhz => XtalFreq::Mhz48,
            _ => XtalFreq::Mhz40,
        }
    }
}

/// How the chip is put into download mode before an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ResetBefore {
    /// Toggle DTR/RTS to reset into the bootloader (auto-reset circuit).
    #[default]
    DefaultReset,
    /// Assume the chip is already in download mode.
    NoReset,
    /// Reset sequence for the built-in USB-Serial-JTAG peripheral.
    UsbReset,
}

impl ResetBefore {
    pub fn to_espflash(self) -> ResetBeforeOperation {
        match self {
            ResetBefore::DefaultReset => ResetBeforeOperation::DefaultReset,
            ResetBefore::NoReset => ResetBeforeOperation::NoReset,
            ResetBefore::UsbReset => ResetBeforeOperation::UsbReset,
        }
    }
}

/// What happens to the chip after an operation completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ResetAfter {
    /// Reset the chip so the application starts.
    #[default]
    HardReset,
    /// Leave the chip in the ROM bootloader.
    NoReset,
    /// Leave the chip in the flasher stub.
    NoResetNoStub,
    /// Reset through the RTC watchdog (USB-OTG and USB-Serial-JTAG setups).
    WatchdogReset,
}

impl ResetAfter {
    pub fn to_espflash(self) -> ResetAfterOperation {
        match self {
            ResetAfter::HardReset => ResetAfterOperation::HardReset,
            ResetAfter::NoReset => ResetAfterOperation::NoReset,
            ResetAfter::NoResetNoStub => ResetAfterOperation::NoResetNoStub,
            ResetAfter::WatchdogReset => ResetAfterOperation::WatchdogReset,
        }
    }
}

/// Baud rates offered in the interface. Any value in
/// [`MIN_BAUD`]..=[`MAX_BAUD`] is accepted by the backend.
pub const COMMON_BAUD_RATES: [u32; 7] = [115_200, 230_400, 460_800, 921_600, 1_500_000, 2_000_000, 3_000_000];
pub const MIN_BAUD: u32 = 300;
pub const MAX_BAUD: u32 = 5_000_000;
/// The ROM bootloader always starts at this rate.
pub const ROM_BAUD: u32 = 115_200;

pub fn validate_baud(baud: u32) -> crate::Result<u32> {
    if (MIN_BAUD..=MAX_BAUD).contains(&baud) {
        Ok(baud)
    } else {
        Err(crate::Error::invalid_input(format!(
            "Baud rate {baud} is outside the supported range ({MIN_BAUD}–{MAX_BAUD})."
        )))
    }
}

/// Parameters used to establish a bootloader connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectionOptions {
    /// Baud rate used after the connection is established.
    pub baud: u32,
    /// Load the RAM flasher stub for faster and more capable operations.
    pub use_stub: bool,
    pub reset_before: ResetBefore,
    pub reset_after: ResetAfter,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            baud: 460_800,
            use_stub: true,
            reset_before: ResetBefore::DefaultReset,
            reset_after: ResetAfter::HardReset,
        }
    }
}

impl ConnectionOptions {
    pub fn validate(&self) -> crate::Result<()> {
        validate_baud(self.baud)?;
        Ok(())
    }
}

/// Optional overrides for the flash configuration written into image
/// headers. `None` means "use the detected or default value".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlashSettings {
    pub mode: Option<FlashMode>,
    pub size: Option<FlashSize>,
    pub frequency: Option<FlashFrequency>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_roundtrip() {
        for chip in Chip::ALL {
            assert_eq!(Chip::from_espflash(chip.to_espflash()), Some(chip));
        }
    }

    #[test]
    fn chip_parse_accepts_common_spellings() {
        assert_eq!(Chip::parse("esp32c3"), Some(Chip::Esp32c3));
        assert_eq!(Chip::parse("ESP32-S3"), Some(Chip::Esp32s3));
        assert_eq!(Chip::parse("esp32_c6"), Some(Chip::Esp32c6));
        assert_eq!(Chip::parse("esp8266"), None);
    }

    #[test]
    fn descriptor_reports_bootloader_offsets() {
        assert_eq!(ChipDescriptor::for_chip(Chip::Esp32).bootloader_offset, 0x1000);
        assert_eq!(ChipDescriptor::for_chip(Chip::Esp32c3).bootloader_offset, 0x0);
        assert_eq!(ChipDescriptor::for_chip(Chip::Esp32p4).bootloader_offset, 0x2000);
    }

    #[test]
    fn descriptor_frequencies_are_supported() {
        let d = ChipDescriptor::for_chip(Chip::Esp32c3);
        assert!(d.supported_flash_frequencies.contains(&d.default_flash_frequency));
    }

    #[test]
    fn flash_size_bytes() {
        assert_eq!(FlashSize::Mb4.bytes(), 4 * 1024 * 1024);
        assert_eq!(FlashSize::from_espflash(FlashSize::Mb16.to_espflash()), Some(FlashSize::Mb16));
    }

    #[test]
    fn baud_validation() {
        assert!(validate_baud(115_200).is_ok());
        assert!(validate_baud(0).is_err());
        assert!(validate_baud(10_000_000).is_err());
    }
}
