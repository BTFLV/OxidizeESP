//! Application services: the boundary used by the user interface layer.
//!
//! [`Services`] owns the hardware backend, the device sessions, running
//! jobs and serial monitors. All methods are synchronous; callers run
//! hardware operations on a blocking worker thread.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, mpsc},
    thread::JoinHandle,
};

use crate::{
    Error, ErrorKind, Result,
    fsutil,
    hardware::Backend,
    job::{self, CancelToken, EventSink, JobContext, JobEvent, JobState, OperationKind},
    monitor::{self, MonitorCommand, MonitorConfig, MonitorRequest, MonitorSink, SymbolResolver},
    ports::SerialPortDescriptor,
    session::{Activity, DeviceManager, DeviceSnapshot, Lease},
};

#[derive(Debug)]
struct JobEntry {
    cancel: CancelToken,
    cancellable: bool,
    state: JobState,
}

struct MonitorHandle {
    device_id: String,
    commands: mpsc::Sender<MonitorCommand>,
    thread: Option<JoinHandle<()>>,
}

/// Whether an operation can be stopped between steps without leaving the
/// device in an undefined state beyond what the user already accepted.
pub fn is_cancellable(kind: OperationKind) -> bool {
    matches!(
        kind,
        OperationKind::ReadFlash | OperationKind::EraseRegion | OperationKind::ErasePartitions
    )
}

pub struct Services {
    backend: Arc<dyn Backend>,
    devices: DeviceManager,
    jobs: Mutex<HashMap<String, JobEntry>>,
    monitors: Arc<Mutex<HashMap<String, MonitorHandle>>>,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services").field("simulated", &self.backend.is_simulated()).finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Services {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            backend,
            devices: DeviceManager::new(),
            jobs: Mutex::new(HashMap::new()),
            monitors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn devices(&self) -> &DeviceManager {
        &self.devices
    }

    pub fn is_simulated(&self) -> bool {
        self.backend.is_simulated()
    }

    /// Enumerate ports and update session presence.
    pub fn list_ports(&self) -> Result<Vec<SerialPortDescriptor>> {
        let ports = self.backend.list_ports()?;
        self.devices.reconcile(&ports);
        Ok(ports)
    }

    /// Open (or reuse) a device session for the named port without
    /// communicating with the device.
    pub fn open_device(&self, port_name: &str) -> Result<DeviceSnapshot> {
        let ports = self.list_ports()?;
        let port = ports
            .into_iter()
            .find(|p| p.name == port_name)
            .ok_or_else(|| {
                Error::new(ErrorKind::Disconnected, format!("Serial port {port_name} was not found."))
                    .with_hint("Refresh the port list and check the USB connection.")
            })?;
        Ok(self.devices.open(port))
    }

    /// Close a device session, stopping its serial monitor first.
    pub fn close_device(&self, device_id: &str) -> Result<()> {
        let monitor_id = lock(&self.monitors)
            .iter()
            .find(|(_, m)| m.device_id == device_id)
            .map(|(id, _)| id.clone());
        if let Some(id) = monitor_id {
            self.stop_monitor(&id)?;
        }
        self.devices.close(device_id)
    }

    /// Run a hardware operation on a device with exclusive access.
    ///
    /// Emits `Started` and exactly one terminal event to `sink`. The closure
    /// runs on the calling thread, which must not be an async executor
    /// thread.
    pub fn run_job<T>(
        &self,
        device_id: &str,
        kind: OperationKind,
        sink: &dyn EventSink,
        operation: impl FnOnce(&mut JobContext<'_>, &dyn Backend, &Lease) -> Result<T>,
        success_message: impl FnOnce(&T) -> String,
    ) -> Result<T> {
        let job_id = uuid::Uuid::new_v4().to_string();
        let lease = self
            .devices
            .acquire(device_id, Activity::Operation { job_id: job_id.clone(), kind })?;
        let cancel = CancelToken::new();
        let cancellable = is_cancellable(kind);
        lock(&self.jobs).insert(
            job_id.clone(),
            JobEntry { cancel: cancel.clone(), cancellable, state: JobState::Running },
        );
        sink.emit(JobEvent::Started { job_id: job_id.clone(), kind, cancellable });

        let mut ctx = JobContext::new(sink, cancel);
        let result = operation(&mut ctx, self.backend.as_ref(), &lease);
        match &result {
            Ok(_) => lease.clear_error(),
            Err(err) if err.kind != ErrorKind::Cancelled => lease.record_error(err),
            Err(_) => {}
        }
        job::finish(&ctx, &result, success_message);

        let terminal = match &result {
            Ok(_) => JobState::Completed,
            Err(e) if e.kind == ErrorKind::Cancelled => JobState::Cancelled,
            Err(_) => JobState::Failed,
        };
        if let Some(entry) = lock(&self.jobs).remove(&job_id)
            && let Err(err) = entry.state.transition(terminal)
        {
            log::warn!("{}", err.message);
        }
        drop(lease);
        result
    }

    /// Request cancellation of a running job.
    pub fn cancel_job(&self, job_id: &str) -> Result<()> {
        let mut jobs = lock(&self.jobs);
        let entry = jobs
            .get_mut(job_id)
            .ok_or_else(|| Error::invalid_input("The operation has already finished."))?;
        if !entry.cancellable {
            return Err(Error::unsupported(
                "This operation cannot be interrupted safely and will run to completion.",
            ));
        }
        if entry.state == JobState::Running {
            entry.state = entry.state.transition(JobState::Cancelling)?;
            entry.cancel.cancel();
        }
        Ok(())
    }

    /// Start a serial monitor on a device. Returns the monitor ID.
    pub fn start_monitor(&self, request: &MonitorRequest, sink: Arc<dyn MonitorSink>) -> Result<String> {
        crate::model::validate_baud(request.baud)?;
        let symbols = match &request.elf_path {
            Some(path) => {
                let path = Path::new(path);
                fsutil::require_absolute(path)?;
                let data = crate::image::read_elf(path)?;
                Some(SymbolResolver::from_elf(&data)?)
            }
            None => None,
        };
        let log_path = request.log_path.as_ref().map(PathBuf::from);
        if let Some(path) = &log_path {
            fsutil::validate_output(path)?;
        }

        let monitor_id = uuid::Uuid::new_v4().to_string();
        let lease = self
            .devices
            .acquire(&request.device_id, Activity::Monitoring { monitor_id: monitor_id.clone() })?;
        let link = match self.backend.open_serial(lease.port(), request.baud) {
            Ok(link) => link,
            Err(err) => {
                lease.record_error(&err);
                return Err(err);
            }
        };
        lease.clear_error();

        let (tx, rx) = mpsc::channel();
        let config = MonitorConfig {
            monitor_id: monitor_id.clone(),
            port: lease.port().name.clone(),
            baud: request.baud,
            symbols,
            log_path,
            reset_on_start: request.reset_on_start,
        };
        let monitors = Arc::clone(&self.monitors);
        let devices = self.devices.clone();
        let thread_monitor_id = monitor_id.clone();
        let device_id = request.device_id.clone();
        let thread = std::thread::Builder::new()
            .name("serial-monitor".into())
            .spawn(move || {
                let failure = monitor::run(link, config, sink.as_ref(), &rx);
                drop(lease);
                if let Some(err) = failure
                    && err.kind == ErrorKind::Disconnected
                {
                    devices.mark_disconnected(&device_id, Some(err));
                }
                // Remove the handle unless `stop_monitor` already did.
                lock(&monitors).remove(&thread_monitor_id);
            })
            .map_err(|e| Error::internal("The monitor thread could not be started.").with_detail(e.to_string()))?;

        lock(&self.monitors).insert(
            monitor_id.clone(),
            MonitorHandle { device_id: request.device_id.clone(), commands: tx, thread: Some(thread) },
        );
        Ok(monitor_id)
    }

    pub fn monitor_command(&self, monitor_id: &str, command: MonitorCommand) -> Result<()> {
        let monitors = lock(&self.monitors);
        let handle = monitors
            .get(monitor_id)
            .ok_or_else(|| Error::invalid_input("The serial monitor is not running."))?;
        handle
            .commands
            .send(command)
            .map_err(|_| Error::invalid_input("The serial monitor is not running."))
    }

    /// Stop a monitor and wait until its port has been closed.
    pub fn stop_monitor(&self, monitor_id: &str) -> Result<()> {
        let handle = lock(&self.monitors).remove(monitor_id);
        let Some(mut handle) = handle else { return Ok(()) };
        let _ = handle.commands.send(MonitorCommand::Stop);
        if let Some(thread) = handle.thread.take() {
            thread
                .join()
                .map_err(|_| Error::internal("The serial monitor terminated unexpectedly."))?;
        }
        Ok(())
    }

    /// Monitor ID running on a device, if any.
    pub fn monitor_for_device(&self, device_id: &str) -> Option<String> {
        lock(&self.monitors)
            .iter()
            .find(|(_, m)| m.device_id == device_id)
            .map(|(id, _)| id.clone())
    }

    /// Stop all monitors. Called when the application exits.
    pub fn shutdown(&self) {
        let ids: Vec<String> = lock(&self.monitors).keys().cloned().collect();
        for id in ids {
            let _ = self.stop_monitor(&id);
        }
    }
}

#[cfg(test)]
mod tests;
