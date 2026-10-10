//! Passive serial port discovery.
//!
//! Enumeration only queries the operating system; ports are never opened
//! here, so refreshing the list cannot reset or otherwise disturb a device.

use serde::{Deserialize, Serialize};
use serialport::{SerialPortInfo, SerialPortType};
use ts_rs::TS;

/// Interface type of a serial port as reported by the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PortKind {
    Usb,
    Pci,
    Bluetooth,
    Unknown,
}

/// USB descriptor information, when the port belongs to a USB device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsbInfo {
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// How likely it is that an ESP device is attached, judged only from USB
/// metadata. This is a hint for sorting and labelling, never a verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PortHint {
    /// Espressif's own USB-Serial-JTAG or USB-OTG interface.
    EspressifUsb,
    /// A USB-to-UART bridge commonly found on ESP development boards.
    UsbUartBridge,
    /// Any other port.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SerialPortDescriptor {
    /// Operating system name of the port (`COM3`, `/dev/cu.usbserial-0001`).
    pub name: String,
    pub kind: PortKind,
    pub usb: Option<UsbInfo>,
    pub hint: PortHint,
    /// Name of the known USB bridge chip, when recognised from VID/PID.
    pub bridge: Option<String>,
}

impl SerialPortDescriptor {
    /// Identity derived from USB metadata that survives port renaming, if the
    /// device reports a serial number.
    pub fn usb_identity(&self) -> Option<String> {
        let usb = self.usb.as_ref()?;
        let serial = usb.serial_number.as_ref().filter(|s| !s.trim().is_empty())?;
        Some(format!("{:04x}:{:04x}:{}", usb.vid, usb.pid, serial.trim()))
    }

    pub fn pid(&self) -> u16 {
        self.usb.as_ref().map_or(0, |u| u.pid)
    }
}

pub const ESPRESSIF_VID: u16 = 0x303A;
/// PID of the built-in USB-Serial-JTAG peripheral.
pub const USB_SERIAL_JTAG_PID: u16 = 0x1001;

fn bridge_name(vid: u16, pid: u16) -> Option<&'static str> {
    Some(match (vid, pid) {
        (ESPRESSIF_VID, USB_SERIAL_JTAG_PID) => "Espressif USB-Serial-JTAG",
        (ESPRESSIF_VID, _) => "Espressif USB",
        (0x10C4, 0xEA60) => "Silicon Labs CP210x",
        (0x10C4, _) => "Silicon Labs",
        (0x1A86, 0x7523) => "WCH CH340",
        (0x1A86, 0x55D4) => "WCH CH9102",
        (0x1A86, _) => "WCH",
        (0x0403, 0x6001) => "FTDI FT232R",
        (0x0403, 0x6010) => "FTDI FT2232",
        (0x0403, 0x6014) => "FTDI FT232H",
        (0x0403, 0x6015) => "FTDI FT-X",
        (0x0403, _) => "FTDI",
        (0x067B, _) => "Prolific PL2303",
        _ => return None,
    })
}

pub fn describe(info: &SerialPortInfo) -> SerialPortDescriptor {
    let (kind, usb) = match &info.port_type {
        SerialPortType::UsbPort(usb) => (
            PortKind::Usb,
            Some(UsbInfo {
                vid: usb.vid,
                pid: usb.pid,
                serial_number: usb.serial_number.clone(),
                manufacturer: usb.manufacturer.clone(),
                product: usb.product.clone(),
            }),
        ),
        SerialPortType::PciPort => (PortKind::Pci, None),
        SerialPortType::BluetoothPort => (PortKind::Bluetooth, None),
        SerialPortType::Unknown => (PortKind::Unknown, None),
    };
    let bridge = usb.as_ref().and_then(|u| bridge_name(u.vid, u.pid));
    let hint = match &usb {
        Some(u) if u.vid == ESPRESSIF_VID => PortHint::EspressifUsb,
        Some(_) if bridge.is_some() => PortHint::UsbUartBridge,
        _ => PortHint::Other,
    };
    SerialPortDescriptor { name: info.port_name.clone(), kind, usb, hint, bridge: bridge.map(str::to_owned) }
}

/// Normalise and sort an enumerated port list.
///
/// On macOS every serial device appears twice, as a `/dev/tty.*` call-in
/// device and a `/dev/cu.*` call-out device. Opening the `tty` variant blocks
/// until carrier detect is asserted, so only the `cu` variant is shown.
pub fn normalize(ports: Vec<SerialPortDescriptor>) -> Vec<SerialPortDescriptor> {
    let mut ports: Vec<_> = ports
        .into_iter()
        .filter(|p| !p.name.starts_with("/dev/tty."))
        .filter(|p| p.kind != PortKind::Bluetooth)
        .collect();
    ports.sort_by(|a, b| rank(a.hint).cmp(&rank(b.hint)).then_with(|| natural_key(&a.name).cmp(&natural_key(&b.name))));
    ports.dedup_by(|a, b| a.name == b.name);
    ports
}

fn rank(hint: PortHint) -> u8 {
    match hint {
        PortHint::EspressifUsb => 0,
        PortHint::UsbUartBridge => 1,
        PortHint::Other => 2,
    }
}

/// Sort `COM10` after `COM9`.
fn natural_key(name: &str) -> (String, u64) {
    let digits: String =
        name.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
    let prefix = name[..name.len() - digits.len()].to_ascii_lowercase();
    (prefix, digits.parse().unwrap_or(0))
}

/// Enumerate the serial ports currently present on the system.
pub fn list_system_ports() -> crate::Result<Vec<SerialPortDescriptor>> {
    let ports = serialport::available_ports().map_err(|e| {
        crate::Error::new(crate::ErrorKind::Io, "Serial ports could not be enumerated.").with_detail(e.to_string())
    })?;
    Ok(normalize(ports.iter().map(describe).collect()))
}

#[cfg(test)]
mod tests {
    use serialport::UsbPortInfo;

    use super::*;

    fn usb(name: &str, vid: u16, pid: u16, serial: Option<&str>) -> SerialPortInfo {
        SerialPortInfo {
            port_name: name.into(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                serial_number: serial.map(Into::into),
                manufacturer: None,
                product: None,
            }),
        }
    }

    #[test]
    fn classifies_known_bridges() {
        let p = describe(&usb("COM3", 0x10C4, 0xEA60, None));
        assert_eq!(p.hint, PortHint::UsbUartBridge);
        assert_eq!(p.bridge.as_deref(), Some("Silicon Labs CP210x"));

        let p = describe(&usb("/dev/cu.usbmodem1101", ESPRESSIF_VID, USB_SERIAL_JTAG_PID, Some("AB:CD")));
        assert_eq!(p.hint, PortHint::EspressifUsb);
        assert_eq!(p.usb_identity().as_deref(), Some("303a:1001:AB:CD"));

        let p = describe(&usb("COM9", 0x1234, 0x5678, Some("  ")));
        assert_eq!(p.hint, PortHint::Other);
        assert_eq!(p.usb_identity(), None);
    }

    #[test]
    fn normalizes_macos_duplicates_and_sorts() {
        let ports = vec![
            describe(&usb("/dev/tty.usbserial-1", 0x10C4, 0xEA60, None)),
            describe(&usb("/dev/cu.usbserial-1", 0x10C4, 0xEA60, None)),
            describe(&SerialPortInfo {
                port_name: "/dev/cu.Bluetooth-Incoming-Port".into(),
                port_type: SerialPortType::BluetoothPort,
            }),
            describe(&SerialPortInfo { port_name: "/dev/cu.debug-console".into(), port_type: SerialPortType::Unknown }),
            describe(&usb("/dev/cu.usbmodem2", ESPRESSIF_VID, USB_SERIAL_JTAG_PID, None)),
        ];
        let names: Vec<_> = normalize(ports).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["/dev/cu.usbmodem2", "/dev/cu.usbserial-1", "/dev/cu.debug-console"]);
    }

    #[test]
    fn sorts_com_ports_naturally() {
        let ports = ["COM10", "COM2", "COM1"]
            .into_iter()
            .map(|n| describe(&SerialPortInfo { port_name: n.into(), port_type: SerialPortType::Unknown }))
            .collect();
        let names: Vec<_> = normalize(ports).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["COM1", "COM2", "COM10"]);
    }
}
