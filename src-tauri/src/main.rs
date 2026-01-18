// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod models;
mod proxy;
mod storage;

use proxy::ProxyServer;
use std::sync::Arc;
use storage::Database;
use tokio::sync::RwLock;

pub struct AppState {
    pub db: Arc<Database>,
    pub proxy: Arc<RwLock<Option<ProxyServer>>>,
}

#[tokio::main]
async fn main() {
    // 로깅 초기화
    tracing_subscriber::fmt::init();

    // 데이터베이스 초기화
    let db = match Database::init().await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    let app_state = AppState {
        db,
        proxy: Arc::new(RwLock::new(None)),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::create_account,
            commands::get_accounts,
            commands::get_active_account,
            commands::switch_account,
            commands::delete_account,
            commands::start_proxy,
            commands::stop_proxy,
            commands::get_proxy_status,
            commands::backup_claude_settings,
            commands::restore_claude_settings,
            commands::list_backups,
            commands::delete_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
