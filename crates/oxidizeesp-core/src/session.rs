//! Device sessions and exclusive port ownership.
//!
//! A session represents one physical device the user is working with. It is
//! bound to a serial port, but its identity (USB serial number, MAC address)
//! is tracked separately so the session can follow the device when the
//! operating system assigns a new port name.
//!
//! Every hardware activity must hold a [`Lease`] on the session. A session
//! has at most one lease at a time, which guarantees that a monitor, a flash
//! write and a backup can never open the same port concurrently. Leases are
//! released automatically when dropped.

use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use time::OffsetDateTime;
use ts_rs::TS;

use crate::{Error, ErrorKind, Result, hardware::ChipInfo, job::OperationKind, ports::SerialPortDescriptor};

/// What a session is currently doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Activity {
    Idle,
    Operation { job_id: String, kind: OperationKind },
    Monitoring { monitor_id: String },
}

/// Overall status shown to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DeviceStatus {
    /// The port is known but the chip has not been identified.
    Unverified,
    /// The chip has been identified and the device is idle.
    Ready,
    /// An operation or the serial monitor is using the device.
    Busy,
    /// The serial port is no longer present.
    Disconnected,
}

/// Serializable view of a session.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeviceSnapshot {
    pub id: String,
    pub port: SerialPortDescriptor,
    pub status: DeviceStatus,
    pub activity: Activity,
    /// Verified chip information; `None` until the device has been probed.
    pub chip_info: Option<ChipInfo>,
    /// RFC 3339 timestamp of the last successful identification.
    pub verified_at: Option<String>,
    /// Stable identity derived from USB metadata, if available.
    pub usb_identity: Option<String>,
    /// Set when the device reappeared under a different port name.
    pub previous_port_name: Option<String>,
    pub last_error: Option<Error>,
}

#[derive(Debug)]
struct Session {
    id: String,
    port: SerialPortDescriptor,
    present: bool,
    activity: Activity,
    chip_info: Option<ChipInfo>,
    verified_at: Option<String>,
    previous_port_name: Option<String>,
    last_error: Option<Error>,
    lease_seq: u64,
}

impl Session {
    fn snapshot(&self) -> DeviceSnapshot {
        let status = if !self.present {
            DeviceStatus::Disconnected
        } else if self.activity != Activity::Idle {
            DeviceStatus::Busy
        } else if self.chip_info.is_some() {
            DeviceStatus::Ready
        } else {
            DeviceStatus::Unverified
        };
        DeviceSnapshot {
            id: self.id.clone(),
            port: self.port.clone(),
            status,
            activity: self.activity.clone(),
            chip_info: self.chip_info.clone(),
            verified_at: self.verified_at.clone(),
            usb_identity: self.port.usb_identity(),
            previous_port_name: self.previous_port_name.clone(),
            last_error: self.last_error.clone(),
        }
    }
}

type ChangeListener = Box<dyn Fn(Vec<DeviceSnapshot>) + Send + Sync>;

#[derive(Default)]
struct Inner {
    sessions: Vec<Session>,
}

/// Registry of device sessions.
#[derive(Clone, Default)]
pub struct DeviceManager {
    inner: Arc<Mutex<Inner>>,
    listener: Arc<Mutex<Option<ChangeListener>>>,
}

impl std::fmt::Debug for DeviceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceManager").finish_non_exhaustive()
    }
}

/// Exclusive access to a device for the lifetime of the value.
#[derive(Debug)]
pub struct Lease {
    manager: DeviceManager,
    device_id: String,
    seq: u64,
    port: SerialPortDescriptor,
    known_info: Option<ChipInfo>,
}

impl Lease {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Port the device was attached to when the lease was taken.
    pub fn port(&self) -> &SerialPortDescriptor {
        &self.port
    }

    /// Chip information known before this lease was taken.
    pub fn known_info(&self) -> Option<&ChipInfo> {
        self.known_info.as_ref()
    }

    /// Record freshly verified chip information.
    pub fn record_info(&self, info: ChipInfo) {
        self.manager.update(&self.device_id, |s| {
            s.chip_info = Some(info);
            s.verified_at = Some(now_rfc3339());
            s.last_error = None;
        });
    }

    pub fn record_error(&self, error: &Error) {
        let error = error.clone();
        self.manager.update(&self.device_id, |s| {
            if error.kind == ErrorKind::Disconnected {
                s.present = false;
            }
            s.last_error = Some(error);
        });
    }

    pub fn clear_error(&self) {
        self.manager.update(&self.device_id, |s| s.last_error = None);
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let seq = self.seq;
        self.manager.update(&self.device_id, |s| {
            if s.lease_seq == seq {
                s.activity = Activity::Idle;
            }
        });
    }
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}

impl DeviceManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a callback invoked with the full device list on every change.
    pub fn set_listener(&self, listener: impl Fn(Vec<DeviceSnapshot>) + Send + Sync + 'static) {
        *self.listener.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(listener));
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn notify(&self) {
        let snapshot = self.list();
        if let Some(listener) = self.listener.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            listener(snapshot);
        }
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut Session)) {
        let changed = {
            let mut inner = self.lock();
            match inner.sessions.iter_mut().find(|s| s.id == id) {
                Some(session) => {
                    f(session);
                    true
                }
                None => false,
            }
        };
        if changed {
            self.notify();
        }
    }

    pub fn list(&self) -> Vec<DeviceSnapshot> {
        self.lock().sessions.iter().map(Session::snapshot).collect()
    }

    pub fn get(&self, id: &str) -> Result<DeviceSnapshot> {
        self.lock().sessions.iter().find(|s| s.id == id).map(Session::snapshot).ok_or_else(unknown_device)
    }

    /// Open a session for `port`, or return the existing session using it.
    pub fn open(&self, port: SerialPortDescriptor) -> DeviceSnapshot {
        let snapshot = {
            let mut inner = self.lock();
            if let Some(existing) = inner.sessions.iter_mut().find(|s| s.port.name == port.name) {
                existing.port = port;
                existing.present = true;
                existing.snapshot()
            } else {
                let session = Session {
                    id: uuid::Uuid::new_v4().to_string(),
                    port,
                    present: true,
                    activity: Activity::Idle,
                    chip_info: None,
                    verified_at: None,
                    previous_port_name: None,
                    last_error: None,
                    lease_seq: 0,
                };
                let snapshot = session.snapshot();
                inner.sessions.push(session);
                snapshot
            }
        };
        self.notify();
        snapshot
    }

    /// Close a session. Fails while the device is in use.
    pub fn close(&self, id: &str) -> Result<()> {
        {
            let mut inner = self.lock();
            let index = inner.sessions.iter().position(|s| s.id == id).ok_or_else(unknown_device)?;
            if inner.sessions[index].activity != Activity::Idle {
                return Err(busy_error(&inner.sessions[index].activity));
            }
            inner.sessions.remove(index);
        }
        self.notify();
        Ok(())
    }

    /// Acquire exclusive access to a device for `activity`.
    pub fn acquire(&self, id: &str, activity: Activity) -> Result<Lease> {
        let lease = {
            let mut inner = self.lock();
            let session = inner.sessions.iter_mut().find(|s| s.id == id).ok_or_else(unknown_device)?;
            if !session.present {
                return Err(Error::new(
                    ErrorKind::Disconnected,
                    format!("The device on {} is disconnected.", session.port.name),
                )
                .with_hint("Reconnect the device and refresh the port list."));
            }
            if session.activity != Activity::Idle {
                return Err(busy_error(&session.activity));
            }
            session.lease_seq += 1;
            session.activity = activity;
            Lease {
                manager: self.clone(),
                device_id: session.id.clone(),
                seq: session.lease_seq,
                port: session.port.clone(),
                known_info: session.chip_info.clone(),
            }
        };
        self.notify();
        Ok(lease)
    }

    /// Update presence and port names from a fresh port enumeration.
    ///
    /// A session whose port vanished is re-associated with a port that
    /// reports the same USB identity. Sessions that are in use are never
    /// moved, because their open handle refers to the original port.
    pub fn reconcile(&self, ports: &[SerialPortDescriptor]) {
        let changed = {
            let mut inner = self.lock();
            let claimed: Vec<String> = inner.sessions.iter().map(|s| s.port.name.clone()).collect();
            let mut changed = false;
            for session in &mut inner.sessions {
                if let Some(port) = ports.iter().find(|p| p.name == session.port.name) {
                    if !session.present || *port != session.port {
                        session.present = true;
                        session.port = port.clone();
                        changed = true;
                    }
                    continue;
                }
                let renamed = session.port.usb_identity().and_then(|identity| {
                    ports
                        .iter()
                        .find(|p| p.usb_identity().as_deref() == Some(identity.as_str()) && !claimed.contains(&p.name))
                });
                match renamed {
                    Some(port) if session.activity == Activity::Idle => {
                        session.previous_port_name = Some(session.port.name.clone());
                        session.port = port.clone();
                        session.present = true;
                        changed = true;
                    }
                    _ => {
                        if session.present {
                            session.present = false;
                            changed = true;
                        }
                    }
                }
            }
            changed
        };
        if changed {
            self.notify();
        }
    }

    /// Mark a session as disconnected (for example after a monitor read
    /// failure).
    pub fn mark_disconnected(&self, id: &str, error: Option<Error>) {
        self.update(id, |s| {
            s.present = false;
            if error.is_some() {
                s.last_error = error;
            }
        });
    }
}

fn unknown_device() -> Error {
    Error::invalid_input("The selected device session no longer exists.").with_hint("Select the device again.")
}

fn busy_error(activity: &Activity) -> Error {
    let message = match activity {
        Activity::Monitoring { .. } => "The serial monitor is using this device.".to_owned(),
        Activity::Operation { kind, .. } => format!("The device is busy: {}.", kind.label().to_lowercase()),
        Activity::Idle => "The device is busy.".to_owned(),
    };
    let hint = match activity {
        Activity::Monitoring { .. } => "Stop the serial monitor first.",
        _ => "Wait for the current operation to finish.",
    };
    Error::new(ErrorKind::PortBusy, message).with_hint(hint)
}

/// Ensure the device answering on the port is the one identified earlier.
pub fn verify_identity(known: Option<&ChipInfo>, current: &ChipInfo) -> Result<()> {
    let Some(known) = known else { return Ok(()) };
    if known.chip != current.chip {
        return Err(Error::new(
            ErrorKind::DeviceMismatch,
            format!("A different device is connected: expected {}, found {}.", known.chip_name, current.chip_name),
        )
        .with_hint("Reconnect the device to identify it again."));
    }
    if let (Some(a), Some(b)) = (&known.mac_address, &current.mac_address)
        && !a.eq_ignore_ascii_case(b)
    {
        return Err(Error::new(
            ErrorKind::DeviceMismatch,
            format!("A different {} is connected (MAC {b}, expected {a}).", current.chip_name),
        )
        .with_hint("Reconnect the device to identify it again."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::{
        hardware::mock::SimDevice,
        model::{Chip, FlashSize},
    };

    fn port(name: &str, serial: &str) -> SerialPortDescriptor {
        SimDevice::new(name, Chip::Esp32c3, FlashSize::Mb4, serial).port
    }

    fn op(kind: OperationKind) -> Activity {
        Activity::Operation { job_id: "job".into(), kind }
    }

    #[test]
    fn open_reuses_session_for_same_port() {
        let m = DeviceManager::new();
        let a = m.open(port("COM3", "A"));
        let b = m.open(port("COM3", "A"));
        assert_eq!(a.id, b.id);
        assert_eq!(m.list().len(), 1);
        assert_eq!(a.status, DeviceStatus::Unverified);
    }

    #[test]
    fn leases_are_exclusive() {
        let m = DeviceManager::new();
        let id = m.open(port("COM3", "A")).id;
        let lease = m.acquire(&id, Activity::Monitoring { monitor_id: "m".into() }).unwrap();
        let err = m.acquire(&id, op(OperationKind::FlashElf)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::PortBusy);
        assert!(err.message.contains("serial monitor"));
        assert_eq!(m.get(&id).unwrap().status, DeviceStatus::Busy);
        drop(lease);
        assert_eq!(m.get(&id).unwrap().activity, Activity::Idle);
        assert!(m.acquire(&id, op(OperationKind::ReadFlash)).is_ok());
    }

    #[test]
    fn distinct_devices_do_not_conflict() {
        let m = DeviceManager::new();
        let a = m.open(port("COM3", "A")).id;
        let b = m.open(port("COM4", "B")).id;
        let _la = m.acquire(&a, op(OperationKind::FlashElf)).unwrap();
        let _lb = m.acquire(&b, op(OperationKind::ReadFlash)).unwrap();
    }

    #[test]
    fn cannot_close_busy_session() {
        let m = DeviceManager::new();
        let id = m.open(port("COM3", "A")).id;
        let lease = m.acquire(&id, op(OperationKind::EraseFlash)).unwrap();
        assert_eq!(m.close(&id).unwrap_err().kind, ErrorKind::PortBusy);
        drop(lease);
        m.close(&id).unwrap();
        assert!(m.list().is_empty());
    }

    #[test]
    fn reconcile_marks_missing_and_follows_renamed_ports() {
        let m = DeviceManager::new();
        let id = m.open(port("/dev/cu.usbserial-1", "SER1")).id;

        m.reconcile(&[]);
        assert_eq!(m.get(&id).unwrap().status, DeviceStatus::Disconnected);
        assert_eq!(m.acquire(&id, op(OperationKind::ReadFlash)).unwrap_err().kind, ErrorKind::Disconnected);

        m.reconcile(&[port("/dev/cu.usbserial-2", "SER1")]);
        let snapshot = m.get(&id).unwrap();
        assert_eq!(snapshot.port.name, "/dev/cu.usbserial-2");
        assert_eq!(snapshot.previous_port_name.as_deref(), Some("/dev/cu.usbserial-1"));
        assert_eq!(snapshot.status, DeviceStatus::Unverified);
    }

    #[test]
    fn reconcile_does_not_move_to_other_devices() {
        let m = DeviceManager::new();
        let id = m.open(port("COM3", "SER1")).id;
        m.reconcile(&[port("COM5", "OTHER")]);
        let snapshot = m.get(&id).unwrap();
        assert_eq!(snapshot.port.name, "COM3");
        assert_eq!(snapshot.status, DeviceStatus::Disconnected);
    }

    #[test]
    fn disconnect_error_marks_session() {
        let m = DeviceManager::new();
        let id = m.open(port("COM3", "A")).id;
        let lease = m.acquire(&id, op(OperationKind::ReadFlash)).unwrap();
        lease.record_error(&Error::new(ErrorKind::Disconnected, "gone"));
        drop(lease);
        assert_eq!(m.get(&id).unwrap().status, DeviceStatus::Disconnected);
    }

    #[test]
    fn listener_is_notified() {
        let m = DeviceManager::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        m.set_listener(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });
        let id = m.open(port("COM3", "A")).id;
        drop(m.acquire(&id, Activity::Idle));
        assert!(count.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn identity_verification() {
        let info = |chip: Chip, mac: &str| ChipInfo {
            chip,
            chip_name: chip.display_name().into(),
            revision: None,
            revision_number: None,
            crystal: crate::model::XtalFreq::Mhz40,
            flash_size: None,
            features: vec![],
            mac_address: Some(mac.into()),
            security: None,
            stub_loaded: true,
        };
        let a = info(Chip::Esp32c3, "aa:bb:cc:dd:ee:ff");
        assert!(verify_identity(None, &a).is_ok());
        assert!(verify_identity(Some(&a), &info(Chip::Esp32c3, "AA:BB:CC:DD:EE:FF")).is_ok());
        assert_eq!(
            verify_identity(Some(&a), &info(Chip::Esp32c3, "00:11:22:33:44:55")).unwrap_err().kind,
            ErrorKind::DeviceMismatch
        );
        assert_eq!(
            verify_identity(Some(&a), &info(Chip::Esp32s3, "aa:bb:cc:dd:ee:ff")).unwrap_err().kind,
            ErrorKind::DeviceMismatch
        );
    }
}
