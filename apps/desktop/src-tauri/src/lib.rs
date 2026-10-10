//! OxidizeESP desktop application.

use std::sync::Arc;

use oxidizeesp_core::{
    hardware::Backend,
    service::Services,
    settings::{LogLevel, SettingsStore},
};
use tauri::{Emitter, Manager, RunEvent};

mod commands;

use commands::{AppMeta, AppServices};

/// espflash version linked into this build, recorded by the build script.
pub const ESPFLASH_VERSION: &str = env!("OXIDIZEESP_ESPFLASH_VERSION");

pub(crate) fn log_filter(level: LogLevel) -> log::LevelFilter {
    match level {
        LogLevel::Error => log::LevelFilter::Error,
        LogLevel::Warn => log::LevelFilter::Warn,
        LogLevel::Info => log::LevelFilter::Info,
        LogLevel::Debug => log::LevelFilter::Debug,
    }
}

fn create_backend() -> Arc<dyn Backend> {
    #[cfg(feature = "mock-hardware")]
    {
        Arc::new(oxidizeesp_core::hardware::mock::MockBackend::with_demo_devices())
    }
    #[cfg(not(feature = "mock-hardware"))]
    {
        Arc::new(oxidizeesp_core::hardware::espflash_backend::EspflashBackend::new())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .level_for("espflash", log::LevelFilter::Info)
                .max_file_size(2 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let (store, settings_warning) = match app.path().app_config_dir() {
                Ok(dir) => SettingsStore::load(dir.join("settings.json")),
                Err(err) => (
                    SettingsStore::in_memory(),
                    Some(format!("The configuration folder is unavailable; settings will not be saved ({err}).")),
                ),
            };
            log::set_max_level(log_filter(store.get().log_level));
            if let Some(warning) = &settings_warning {
                log::warn!("{warning}");
            }

            let services: AppServices = Arc::new(Services::new(create_backend()));
            if services.is_simulated() {
                log::warn!("Running with simulated hardware; no real devices are accessed.");
            }
            let handle = app.handle().clone();
            services.devices().set_listener(move |devices| {
                if let Err(err) = handle.emit("devices-changed", devices) {
                    log::debug!("devices-changed could not be emitted: {err}");
                }
            });

            app.manage(store);
            app.manage(services);
            app.manage(AppMeta { settings_warning });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::chip_descriptors,
            commands::list_ports,
            commands::list_devices,
            commands::open_device,
            commands::close_device,
            commands::probe_device,
            commands::reset_device,
            commands::hold_in_reset,
            commands::flash_elf,
            commands::write_bin,
            commands::read_flash,
            commands::read_partition_table,
            commands::erase,
            commands::checksum_md5,
            commands::cancel_job,
            commands::inspect_file,
            commands::inspect_elf,
            commands::file_md5,
            commands::verify_backup,
            commands::suggest_backup_name,
            commands::load_partition_table,
            commands::export_partition_table,
            commands::preview_image,
            commands::open_log_folder,
            commands::save_image,
            commands::start_monitor,
            commands::monitor_send,
            commands::monitor_reset,
            commands::stop_monitor,
            commands::save_text_file,
            commands::get_settings,
            commands::update_settings,
            commands::reveal_path,
        ])
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            eprintln!("OxidizeESP failed to start: {err}");
            std::process::exit(1);
        }
    };

    app.run(|handle, event| {
        if let RunEvent::Exit = event
            && let Some(services) = handle.try_state::<AppServices>()
        {
            services.shutdown();
        }
    });
}
