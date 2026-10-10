//! Workflow tests against the simulated backend.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use super::*;
use crate::{
    hardware::{
        ChipInfo,
        mock::{MockBackend, SharedDevice, SimDevice},
    },
    image::ImageOptions,
    job::JobEvent,
    model::{Chip, ConnectionOptions, FlashSize},
    monitor::{MonitorEvent, MonitorRequest},
    ops::{
        self, ChecksumRequest, EraseRequest, EraseTarget, FlashElfRequest, PartitionSource, ReadFlashRequest,
        WriteBinRequest,
    },
    region::FlashRegion,
    session::DeviceStatus,
    testutil::synthetic_esp32c3_elf,
};

#[derive(Default)]
struct Events(Mutex<Vec<JobEvent>>);

impl EventSink for Events {
    fn emit(&self, event: JobEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl Events {
    fn take(&self) -> Vec<JobEvent> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

struct Fixture {
    services: Arc<Services>,
    device: SharedDevice,
    device_id: String,
    dir: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let backend = MockBackend::new();
    let device = backend.add(SimDevice::new("SIMULATED-1", Chip::Esp32c3, FlashSize::Mb4, "SER-1"));
    let services = Arc::new(Services::new(Arc::new(backend)));
    let device_id = services.open_device("SIMULATED-1").unwrap().id;
    Fixture { services, device, device_id, dir: tempfile::tempdir().unwrap() }
}

impl Fixture {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn probe(&self) -> ChipInfo {
        let events = Events::default();
        self.services
            .run_job(
                &self.device_id,
                OperationKind::Probe,
                &events,
                |ctx, b, l| ops::probe(ctx, b, l, &ConnectionOptions::default()),
                |_| "ok".into(),
            )
            .unwrap()
    }

    fn write_bin(&self, path: &Path, offset: u32) -> Result<ops::FlashResult> {
        let request = WriteBinRequest {
            device_id: self.device_id.clone(),
            path: path.display().to_string(),
            offset,
            connection: ConnectionOptions::default(),
            verify: true,
            skip_unchanged: false,
            confirmed: true,
        };
        let events = Events::default();
        self.services.run_job(
            &self.device_id,
            OperationKind::WriteBin,
            &events,
            |ctx, b, l| ops::write_bin(ctx, b, l, &request),
            |_| "ok".into(),
        )
    }

    fn checksum(&self, region: FlashRegion) -> String {
        let request =
            ChecksumRequest { device_id: self.device_id.clone(), region, connection: ConnectionOptions::default() };
        let events = Events::default();
        self.services
            .run_job(
                &self.device_id,
                OperationKind::ChecksumMd5,
                &events,
                |ctx, b, l| ops::checksum(ctx, b, l, &request),
                |_| "ok".into(),
            )
            .unwrap()
            .md5
    }

    fn erase(&self, target: EraseTarget, confirmed: bool) -> Result<ops::EraseResult> {
        let request = EraseRequest {
            device_id: self.device_id.clone(),
            target,
            connection: ConnectionOptions::default(),
            confirmed,
        };
        let events = Events::default();
        self.services.run_job(
            &self.device_id,
            OperationKind::EraseRegion,
            &events,
            |ctx, b, l| ops::erase(ctx, b, l, &request),
            |_| "ok".into(),
        )
    }
}

#[test]
fn probe_identifies_device_and_resets_it() {
    let f = fixture();
    let info = f.probe();
    assert_eq!(info.chip, Chip::Esp32c3);
    assert_eq!(info.flash_size, Some(FlashSize::Mb4));
    let snapshot = f.services.devices().get(&f.device_id).unwrap();
    assert_eq!(snapshot.status, DeviceStatus::Ready);
    assert_eq!(snapshot.chip_info.unwrap().mac_address, info.mac_address);
    assert_eq!(f.device.lock().unwrap().resets, 1);
}

#[test]
fn job_emits_started_progress_and_completion() {
    let f = fixture();
    f.probe();
    let data: Vec<u8> = (0..0x2_0000u32).map(|i| i as u8).collect();
    let file = f.path("fw.bin");
    std::fs::write(&file, &data).unwrap();

    let request = WriteBinRequest {
        device_id: f.device_id.clone(),
        path: file.display().to_string(),
        offset: 0x1_0000,
        connection: ConnectionOptions::default(),
        verify: true,
        skip_unchanged: false,
        confirmed: true,
    };
    let events = Events::default();
    let result = f
        .services
        .run_job(
            &f.device_id,
            OperationKind::WriteBin,
            &events,
            |ctx, b, l| ops::write_bin(ctx, b, l, &request),
            |_| "Done".into(),
        )
        .unwrap();
    assert!(result.verified);
    assert_eq!(result.bytes_written, data.len() as u64);

    let events = events.take();
    assert!(matches!(events.first(), Some(JobEvent::Started { cancellable: false, .. })));
    assert!(events.iter().any(|e| matches!(e, JobEvent::Progress { .. })));
    assert!(events.iter().any(|e| matches!(e, JobEvent::Stage { stage: crate::job::Stage::Verifying, .. })));
    assert!(matches!(events.last(), Some(JobEvent::Completed { .. })));
    let overall: Vec<f64> = events
        .iter()
        .filter_map(|e| match e {
            JobEvent::Progress { overall, .. } => Some(*overall),
            _ => None,
        })
        .collect();
    assert!(overall.windows(2).all(|w| w[0] <= w[1]), "progress must be monotonic: {overall:?}");
    assert_eq!(overall.last().copied(), Some(1.0));

    let flash = &f.device.lock().unwrap().flash;
    assert_eq!(&flash[0x1_0000..0x1_0000 + data.len()], &data[..]);
}

#[test]
fn backup_roundtrip_matches_device_checksum() {
    let f = fixture();
    f.probe();
    let data: Vec<u8> = (0..0x5_0000u32).map(|i| (i * 7) as u8).collect();
    let file = f.path("fw.bin");
    std::fs::write(&file, &data).unwrap();
    f.write_bin(&file, 0x2_0000).unwrap();

    let out = f.path("backup.bin");
    let region = FlashRegion::new(0x2_0000, data.len() as u32);
    let request = ReadFlashRequest {
        device_id: f.device_id.clone(),
        region: Some(region),
        output_path: out.display().to_string(),
        connection: ConnectionOptions::default(),
    };
    let events = Events::default();
    let result = f
        .services
        .run_job(
            &f.device_id,
            OperationKind::ReadFlash,
            &events,
            |ctx, b, l| ops::read_flash(ctx, b, l, &request),
            |_| "ok".into(),
        )
        .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
    assert_eq!(result.metadata.sha256, crate::fsutil::sha256_hex(&data));
    assert_eq!(result.metadata.chip, Chip::Esp32c3);
    assert!(!result.metadata.full_flash);
    let sidecar = crate::backup::read_sidecar(&out).unwrap().unwrap();
    assert_eq!(sidecar, result.metadata);
    assert_eq!(crate::backup::verify(&out).unwrap().matches, Some(true));

    let device_md5 = f.checksum(region);
    let local_md5 = {
        use md5::{Digest, Md5};
        crate::fsutil::hex(&Md5::digest(&data))
    };
    assert_eq!(device_md5, local_md5);

    // The backup can be written back as an input file.
    {
        let mut d = f.device.lock().unwrap();
        d.flash[0x2_0000..0x2_0010].fill(0);
    }
    f.write_bin(&out, 0x2_0000).unwrap();
    assert_eq!(f.checksum(region), local_md5);
}

#[test]
fn full_backup_uses_detected_flash_size() {
    let f = fixture();
    f.probe();
    let out = f.path("full.bin");
    let request = ReadFlashRequest {
        device_id: f.device_id.clone(),
        region: None,
        output_path: out.display().to_string(),
        connection: ConnectionOptions::default(),
    };
    let events = Events::default();
    let result = f
        .services
        .run_job(
            &f.device_id,
            OperationKind::ReadFlash,
            &events,
            |ctx, b, l| ops::read_flash(ctx, b, l, &request),
            |_| "ok".into(),
        )
        .unwrap();
    assert!(result.metadata.full_flash);
    assert_eq!(std::fs::metadata(&out).unwrap().len(), 4 * 1024 * 1024);
}

#[test]
fn cancelled_backup_leaves_no_file() {
    let f = fixture();
    f.probe();
    let out = f.path("cancelled.bin");
    let request = ReadFlashRequest {
        device_id: f.device_id.clone(),
        region: None,
        output_path: out.display().to_string(),
        connection: ConnectionOptions::default(),
    };
    let services = f.services.clone();
    let events = Events::default();
    let result = f.services.run_job(
        &f.device_id,
        OperationKind::ReadFlash,
        &events,
        |ctx, b, l| {
            // Simulate the user pressing "Cancel" while the job runs.
            if let Ok(Activity::Operation { job_id, .. }) = services.devices().get(l.device_id()).map(|s| s.activity) {
                services.cancel_job(&job_id).unwrap();
            }
            ops::read_flash(ctx, b, l, &request)
        },
        |_| "ok".into(),
    );
    assert_eq!(result.unwrap_err().kind, ErrorKind::Cancelled);
    assert!(!out.exists());
    assert_eq!(std::fs::read_dir(f.dir.path()).unwrap().count(), 0, "no temporary files may remain");
    let events = events.take();
    assert!(matches!(events.last(), Some(JobEvent::Cancelled { flash_may_be_inconsistent: false, .. })));
}

#[test]
fn non_cancellable_jobs_reject_cancellation() {
    let f = fixture();
    let events = Events::default();
    let services = f.services.clone();
    f.services
        .run_job(
            &f.device_id,
            OperationKind::FlashElf,
            &events,
            |_, _, l| {
                if let Ok(Activity::Operation { job_id, .. }) =
                    services.devices().get(l.device_id()).map(|s| s.activity)
                {
                    let err = services.cancel_job(&job_id).unwrap_err();
                    assert_eq!(err.kind, ErrorKind::Unsupported);
                }
                Ok(())
            },
            |_| "ok".into(),
        )
        .unwrap();
}

#[test]
fn erase_region_and_partitions() {
    let f = fixture();
    f.probe();
    f.device.lock().unwrap().flash.fill(0);

    let result = f.erase(EraseTarget::Region { region: FlashRegion::new(0x1000, 0x2000) }, true).unwrap();
    assert_eq!(result.regions.unwrap(), vec![FlashRegion::new(0x1000, 0x2000)]);
    {
        let d = f.device.lock().unwrap();
        assert!(d.flash[0x1000..0x3000].iter().all(|b| *b == 0xFF));
        assert_eq!(d.flash[0x0FFF], 0);
        assert_eq!(d.flash[0x3000], 0);
    }

    // Unaligned regions are rejected before connecting.
    let connects = f.device.lock().unwrap().connect_count;
    let err = f.erase(EraseTarget::Region { region: FlashRegion::new(0x1001, 0x1000) }, true).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
    assert_eq!(f.device.lock().unwrap().connect_count, connects);

    // Named partitions from a CSV file.
    let csv = f.path("partitions.csv");
    std::fs::write(&csv, "nvs, data, nvs, 0x9000, 0x6000,\nfactory, app, factory, 0x10000, 0x100000,\n").unwrap();
    let result = f
        .erase(
            EraseTarget::Partitions {
                names: vec!["nvs".into()],
                source: PartitionSource::File { path: csv.display().to_string() },
            },
            true,
        )
        .unwrap();
    assert_eq!(result.partitions.len(), 1);
    let d = f.device.lock().unwrap();
    assert!(d.flash[0x9000..0xF000].iter().all(|b| *b == 0xFF));
    assert_eq!(d.flash[0xF000], 0);
}

#[test]
fn erase_partitions_from_device_table() {
    let f = fixture();
    f.probe();
    let (table, _) =
        crate::partitions::parse(b"nvs, data, nvs, 0x9000, 0x6000,\nfactory, app, factory, 0x10000, 0x100000,\n")
            .unwrap();
    let bin = crate::partitions::to_binary(&table).unwrap();
    {
        let mut d = f.device.lock().unwrap();
        d.flash.fill(0);
        d.flash[0x8000..0x8000 + bin.len()].copy_from_slice(&bin);
        d.flash[0x8000 + bin.len()..0x8C00].fill(0xFF);
    }
    let result = f
        .erase(
            EraseTarget::Partitions { names: vec!["nvs".into()], source: PartitionSource::Device { offset: 0x8000 } },
            true,
        )
        .unwrap();
    assert_eq!(result.partitions[0].name, "nvs");
    assert!(f.device.lock().unwrap().flash[0x9000..0xF000].iter().all(|b| *b == 0xFF));

    let err = f
        .erase(
            EraseTarget::Partitions {
                names: vec!["missing".into()],
                source: PartitionSource::Device { offset: 0x8000 },
            },
            true,
        )
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

#[test]
fn destructive_operations_require_confirmation() {
    let f = fixture();
    let err = f.erase(EraseTarget::Full, false).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
    assert_eq!(f.device.lock().unwrap().connect_count, 0);

    f.device.lock().unwrap().flash.fill(0);
    f.erase(EraseTarget::Full, true).unwrap();
    assert!(f.device.lock().unwrap().flash.iter().all(|b| *b == 0xFF));
}

#[test]
fn write_bin_validates_inputs() {
    let f = fixture();
    f.probe();
    let file = f.path("fw.bin");
    std::fs::write(&file, [1u8; 100]).unwrap();
    assert_eq!(f.write_bin(&file, 0x1001).unwrap_err().kind, ErrorKind::InvalidInput);
    assert_eq!(f.write_bin(&file, 0x40_0000).unwrap_err().kind, ErrorKind::InvalidInput);

    let elf = f.path("app.elf");
    std::fs::write(&elf, synthetic_esp32c3_elf(64)).unwrap();
    assert_eq!(f.write_bin(&elf, 0x1_0000).unwrap_err().kind, ErrorKind::InvalidFile);

    // ESP image header for a different chip (ESP32-S3 has chip ID 9).
    let mut image = vec![0u8; 64];
    image[0] = 0xE9;
    image[12] = 9;
    let wrong = f.path("s3.bin");
    std::fs::write(&wrong, &image).unwrap();
    assert_eq!(f.write_bin(&wrong, 0x0).unwrap_err().kind, ErrorKind::IncompatibleFirmware);

    // Unaligned length is padded with 0xFF.
    std::fs::write(&file, [1u8; 5]).unwrap();
    f.write_bin(&file, 0x1000).unwrap();
    let d = f.device.lock().unwrap();
    assert_eq!(&d.flash[0x1000..0x1008], &[1, 1, 1, 1, 1, 0xFF, 0xFF, 0xFF]);
}

#[test]
fn flash_elf_writes_bootloader_partition_table_and_app() {
    let f = fixture();
    f.probe();
    let elf = f.path("app.elf");
    std::fs::write(&elf, synthetic_esp32c3_elf(4096)).unwrap();
    let request = FlashElfRequest {
        device_id: f.device_id.clone(),
        elf_path: elf.display().to_string(),
        connection: ConnectionOptions::default(),
        image: ImageOptions::default(),
        verify: true,
        skip_unchanged: false,
        erase_partitions: vec![],
        confirmed: true,
    };
    let events = Events::default();
    let result = f
        .services
        .run_job(
            &f.device_id,
            OperationKind::FlashElf,
            &events,
            |ctx, b, l| ops::flash_elf(ctx, b, l, &request),
            |_| "ok".into(),
        )
        .unwrap();
    let summary = result.image.unwrap();
    let offsets: Vec<u32> = summary.segments.iter().map(|s| s.offset).collect();
    assert_eq!(offsets, vec![0x0, 0x8000, 0x1_0000]);
    let d = f.device.lock().unwrap();
    assert_eq!(d.flash[0], 0xE9, "bootloader image magic");
    assert_eq!(&d.flash[0x8000..0x8002], &[0xAA, 0x50], "partition table magic");
    assert_eq!(d.flash[0x1_0000], 0xE9, "application image magic");
}

#[test]
fn flash_elf_requires_app_descriptor_when_checked() {
    let f = fixture();
    let elf = f.path("app.elf");
    std::fs::write(&elf, synthetic_esp32c3_elf(256)).unwrap();
    let request = FlashElfRequest {
        device_id: f.device_id.clone(),
        elf_path: elf.display().to_string(),
        connection: ConnectionOptions::default(),
        image: ImageOptions { check_app_descriptor: true, ..ImageOptions::default() },
        verify: true,
        skip_unchanged: false,
        erase_partitions: vec![],
        confirmed: true,
    };
    let events = Events::default();
    let err = f
        .services
        .run_job(
            &f.device_id,
            OperationKind::FlashElf,
            &events,
            |ctx, b, l| ops::flash_elf(ctx, b, l, &request),
            |_| "ok".into(),
        )
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidFile);
    assert!(f.device.lock().unwrap().flash.iter().all(|b| *b == 0xFF), "nothing may be written");
    assert!(matches!(events.take().last(), Some(JobEvent::Failed { flash_may_be_inconsistent: false, .. })));
}

#[test]
fn swapped_device_is_detected() {
    let f = fixture();
    f.probe();
    // Replace the attached board with a different ESP32-C3.
    f.device.lock().unwrap().mac = "02:00:de:ad:be:ef".into();
    let file = f.path("fw.bin");
    std::fs::write(&file, [1u8; 16]).unwrap();
    let err = f.write_bin(&file, 0x1_0000).unwrap_err();
    assert_eq!(err.kind, ErrorKind::DeviceMismatch);
    assert!(f.device.lock().unwrap().flash[0x1_0000..0x1_0010].iter().all(|b| *b == 0xFF));
}

#[test]
fn monitor_holds_the_port_exclusively() {
    let f = fixture();
    let request = MonitorRequest {
        device_id: f.device_id.clone(),
        baud: 115_200,
        elf_path: None,
        log_path: Some(f.path("monitor.log").display().to_string()),
        reset_on_start: false,
    };
    let lines = Arc::new(Mutex::new(0usize));
    let counter = lines.clone();
    let sink: Arc<dyn MonitorSink> = Arc::new(move |event: MonitorEvent| {
        if let MonitorEvent::Lines { lines } = event {
            *counter.lock().unwrap() += lines.len();
        }
    });
    let monitor_id = f.services.start_monitor(&request, sink.clone()).unwrap();
    assert_eq!(f.services.monitor_for_device(&f.device_id).as_deref(), Some(monitor_id.as_str()));

    let file = f.path("fw.bin");
    std::fs::write(&file, [1u8; 16]).unwrap();
    let err = f.write_bin(&file, 0x1_0000).unwrap_err();
    assert_eq!(err.kind, ErrorKind::PortBusy);
    assert_eq!(f.services.start_monitor(&request, sink).unwrap_err().kind, ErrorKind::PortBusy);

    f.services.monitor_command(&monitor_id, MonitorCommand::Reset).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    f.services.stop_monitor(&monitor_id).unwrap();
    assert!(*lines.lock().unwrap() > 0);
    assert!(f.services.monitor_for_device(&f.device_id).is_none());
    assert!(std::fs::read_to_string(f.path("monitor.log")).unwrap().contains("ESP-ROM"));

    // The port is free again.
    f.write_bin(&file, 0x1_0000).unwrap();
}

#[test]
fn unplugging_during_monitor_marks_device_disconnected() {
    let f = fixture();
    let request = MonitorRequest {
        device_id: f.device_id.clone(),
        baud: 115_200,
        elf_path: None,
        log_path: None,
        reset_on_start: false,
    };
    let stopped = Arc::new(Mutex::new(None));
    let s = stopped.clone();
    let sink: Arc<dyn MonitorSink> = Arc::new(move |event: MonitorEvent| {
        if let MonitorEvent::Stopped { error } = event {
            *s.lock().unwrap() = Some(error);
        }
    });
    f.services.start_monitor(&request, sink).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    f.device.lock().unwrap().plugged_in = false;
    for _ in 0..50 {
        if stopped.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let error = stopped.lock().unwrap().clone().flatten().unwrap();
    assert_eq!(error.kind, ErrorKind::Disconnected);
    assert_eq!(f.services.devices().get(&f.device_id).unwrap().status, DeviceStatus::Disconnected);

    // Replugging makes the device available again after a port refresh.
    f.device.lock().unwrap().plugged_in = true;
    f.services.list_ports().unwrap();
    assert_ne!(f.services.devices().get(&f.device_id).unwrap().status, DeviceStatus::Disconnected);
}

#[test]
fn connection_failures_are_recorded_on_the_session() {
    let f = fixture();
    f.device.lock().unwrap().fail_connect = Some(Error::new(ErrorKind::Connection, "No bootloader response"));
    let events = Events::default();
    let err = f
        .services
        .run_job(
            &f.device_id,
            OperationKind::Probe,
            &events,
            |ctx, b, l| ops::probe(ctx, b, l, &ConnectionOptions::default()),
            |_| "ok".into(),
        )
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Connection);
    let snapshot = f.services.devices().get(&f.device_id).unwrap();
    assert_eq!(snapshot.last_error.map(|e| e.kind), Some(ErrorKind::Connection));
    assert_eq!(snapshot.status, DeviceStatus::Unverified);
    assert!(matches!(events.take().last(), Some(JobEvent::Failed { .. })));
}

#[test]
fn close_device_stops_monitor() {
    let f = fixture();
    let request = MonitorRequest {
        device_id: f.device_id.clone(),
        baud: 115_200,
        elf_path: None,
        log_path: None,
        reset_on_start: true,
    };
    let sink: Arc<dyn MonitorSink> = Arc::new(|_: MonitorEvent| {});
    f.services.start_monitor(&request, sink).unwrap();
    f.services.close_device(&f.device_id).unwrap();
    assert!(f.services.devices().list().is_empty());
    assert!(f.services.monitor_for_device(&f.device_id).is_none());
}

#[test]
fn opening_unknown_port_fails() {
    let f = fixture();
    assert_eq!(f.services.open_device("COM99").unwrap_err().kind, ErrorKind::Disconnected);
}
