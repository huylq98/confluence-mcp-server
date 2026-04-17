#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use configurator::commands;

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_existing_config,
            commands::test_connection,
            commands::save_config,
            commands::remove_config,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Tauri app");
}
