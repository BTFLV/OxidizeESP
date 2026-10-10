use std::path::Path;

/// Commands exposed to the frontend. Each one gets an `allow-<name>`
/// permission that is granted explicitly in `capabilities/main.json`.
const COMMANDS: &[&str] = &[
    "app_info",
    "chip_descriptors",
    "list_ports",
    "list_devices",
    "open_device",
    "close_device",
    "probe_device",
    "reset_device",
    "hold_in_reset",
    "flash_elf",
    "write_bin",
    "read_flash",
    "read_partition_table",
    "erase",
    "checksum_md5",
    "cancel_job",
    "inspect_file",
    "inspect_elf",
    "file_md5",
    "verify_backup",
    "suggest_backup_name",
    "load_partition_table",
    "export_partition_table",
    "preview_image",
    "open_log_folder",
    "save_image",
    "start_monitor",
    "monitor_send",
    "monitor_reset",
    "stop_monitor",
    "save_text_file",
    "get_settings",
    "update_settings",
    "reveal_path",
];

/// Read the resolved espflash version from the workspace lock file so the
/// About section can show it.
fn espflash_version() -> String {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let Ok(contents) = std::fs::read_to_string(&lock) else {
        return "unknown".into();
    };
    let mut lines = contents.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"espflash\""
            && let Some(version) = lines.next().and_then(|l| l.trim().strip_prefix("version = "))
        {
            return version.trim_matches('"').to_owned();
        }
    }
    "unknown".into()
}

fn main() {
    println!("cargo:rustc-env=OXIDIZEESP_ESPFLASH_VERSION={}", espflash_version());
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run the Tauri build script");
}
