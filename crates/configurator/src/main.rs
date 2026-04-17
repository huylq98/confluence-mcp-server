#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use configurator::commands;

#[cfg(windows)]
fn check_webview2_or_warn() -> bool {
    use std::path::PathBuf;

    let candidates = [
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from).map(|p| p.join("Microsoft").join("EdgeWebView").join("Application")),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from).map(|p| p.join("Microsoft").join("Edge").join("Application")),
        std::env::var_os("ProgramFiles").map(PathBuf::from).map(|p| p.join("Microsoft").join("EdgeWebView").join("Application")),
        std::env::var_os("ProgramFiles").map(PathBuf::from).map(|p| p.join("Microsoft").join("Edge").join("Application")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if let Ok(entries) = std::fs::read_dir(&candidate) {
            for entry in entries.flatten() {
                if entry.path().join("msedgewebview2.exe").is_file() {
                    return true;
                }
            }
        }
    }
    // Fallback: check registry
    use winreg::enums::*;
    use winreg::RegKey;
    for (hive, subkey) in [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
        (HKEY_CURRENT_USER,  r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
        (HKEY_CURRENT_USER,  r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
    ] {
        if RegKey::predef(hive).open_subkey(subkey).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn show_missing_webview2_message() {
    use winapi::um::winuser::{MessageBoxW, MB_ICONWARNING};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let text: Vec<u16> = OsStr::new(
        "Microsoft WebView2 Runtime is required but not installed.\n\n\
         Please download it from:\n\
         https://developer.microsoft.com/en-us/microsoft-edge/webview2/\n\n\
         Install the 'Evergreen Bootstrapper' and try again."
    ).encode_wide().chain(Some(0)).collect();
    let caption: Vec<u16> = OsStr::new("Confluence Connect — Missing Component")
        .encode_wide().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONWARNING);
    }
}

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    #[cfg(windows)]
    {
        if !check_webview2_or_warn() {
            show_missing_webview2_message();
            std::process::exit(1);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_existing_config,
            commands::test_connection,
            commands::save_config,
            commands::remove_config,
            commands::server_status,
            commands::stop_server,
            commands::get_stats,
            commands::get_recommendations,
            commands::test_live_connection,
            commands::copy_diagnostics,
            commands::open_claude_log,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Tauri app");
}
