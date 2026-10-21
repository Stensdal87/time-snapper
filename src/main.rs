// Skjul konsolvinduet på Windows i release-builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod data;
mod entry_editor;
mod export;
mod export_dialog;
mod idle;

use eframe::egui;

const APP_NAME: &str = "Time Snapper";

fn main() -> eframe::Result {
    migrate_from_old_name();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([320.0, 520.0])
            .with_min_inner_size([260.0, 300.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default())
            .with_always_on_top(),
        ..Default::default()
    };
    // Wayland lader ikke programmer lægge sig øverst, så på Linux kører vi via
    // X11/XWayland, hvor vinduesmanageren respekterer det.
    #[cfg(target_os = "linux")]
    let options = eframe::NativeOptions {
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        })),
        ..options
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::TimeSnapperApp::new(&cc.egui_ctx, data::Data::default_path())))),
    )
}

/// Programmet hed tidligere "Time Tracker". Kopiér data og vinduesindstillinger
/// til de nye placeringer, hvis de ikke findes der endnu. De gamle filer
/// bliver liggende som sikkerhedskopi.
fn migrate_from_old_name() {
    let mut files = vec![(data::Data::path_for("TimeTracker"), data::Data::default_path())];
    if let (Some(old), Some(new)) = (eframe::storage_dir("Time Tracker"), eframe::storage_dir(APP_NAME)) {
        files.push((old.join("app.ron"), new.join("app.ron")));
    }
    for (old, new) in files {
        if old.exists() && !new.exists() {
            if let Some(dir) = new.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::copy(&old, &new);
        }
    }
}
