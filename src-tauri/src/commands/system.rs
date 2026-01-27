use serde::Serialize;

#[allow(dead_code)]
#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub status: String,
}

/// Test command - returns "pong" with timestamp
#[tauri::command]
pub fn ping() -> String {
    let now = chrono::Local::now();
    format!("pong @ {}", now.format("%H:%M:%S"))
}

/// Returns application information
#[allow(dead_code)]
#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "FileManagerDaz".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        status: "Skeleton OK".to_string(),
    }
}
