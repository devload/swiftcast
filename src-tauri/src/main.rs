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

    let proxy = Arc::new(RwLock::new(None));

    // 자동 시작 설정 확인 및 프록시 시작
    let auto_start = db.get_auto_start().await.unwrap_or(true);
    if auto_start {
        let port = db.get_proxy_port().await.unwrap_or(32080);
        let mut server = ProxyServer::new(db.clone());
        if let Err(e) = server.start(port).await {
            tracing::error!("Failed to auto-start proxy: {}", e);
        } else {
            tracing::info!("Proxy auto-started on port {}", port);
            *proxy.write().await = Some(server);
        }
    }

    let app_state = AppState {
        db,
        proxy,
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
            commands::get_claude_token_from_keychain,
            commands::auto_scan_accounts,
            commands::get_usage_stats,
            commands::get_app_config,
            commands::set_proxy_port,
            commands::set_auto_start,
            commands::get_proxy_port,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
