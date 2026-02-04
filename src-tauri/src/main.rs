// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod models;
mod proxy;
mod storage;

use proxy::ProxyServer;
use std::sync::Arc;
use storage::Database;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tokio::sync::RwLock;

pub struct AppState {
    pub db: Arc<Database>,
    pub proxy: Arc<RwLock<Option<ProxyServer>>>,
}

#[tokio::main]
async fn main() {
    // 로깅 초기화 (콘솔 + 파일)
    let log_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.swiftcast.app")
        .join("logs");

    // 로그 디렉토리 생성
    let _ = std::fs::create_dir_all(&log_dir);

    // 파일 로거 설정 (일별 회전)
    let file_appender = tracing_appender::rolling::daily(&log_dir, "swiftcast.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 콘솔 + 파일 로깅
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_target(false)
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_writer(non_blocking)
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    tracing::info!("SwiftCast starting... Log file: {:?}", log_dir.join("swiftcast.log"));

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
    let port = db.get_proxy_port().await.unwrap_or(32080);

    if auto_start {
        let mut server = ProxyServer::new(db.clone());
        if let Err(e) = server.start(port).await {
            tracing::error!("Failed to auto-start proxy: {}", e);
        } else {
            tracing::info!("Proxy auto-started on port {}", port);
            *proxy.write().await = Some(server);
        }
    }

    // 활성 계정에 따라 Claude settings.json 업데이트
    if let Ok(Some(account)) = db.get_active_account().await {
        if let Err(e) = commands::init_claude_settings(&account.base_url, port) {
            tracing::error!("Failed to init Claude settings: {}", e);
        } else {
            tracing::info!("Claude settings initialized for account: {}", account.name);
        }
    }

    let app_state = AppState {
        db,
        proxy,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 이미 실행 중인 인스턴스가 있으면 기존 창을 활성화
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
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
            // 사용량 관련
            commands::get_recent_usage,
            commands::get_usage_by_account,
            commands::get_usage_by_model,
            commands::get_daily_usage,
            commands::get_usage_by_session,
            commands::clear_usage_logs,
            // 세션 관리
            commands::get_available_models,
            commands::get_active_sessions,
            commands::set_session_config,
            commands::delete_session_config,
            // Hook 설정
            commands::get_hook_config,
            commands::set_hook_config,
            commands::get_app_version,
            // 세션별 Hook 설정
            commands::get_session_hooks,
            commands::set_session_hooks,
            commands::delete_session_hooks,
            commands::get_all_session_hooks,
        ])
        .setup(|app| {
            // 트레이 메뉴 생성
            let show_i = MenuItem::with_id(app, "show", "SwiftCast 열기", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &separator, &quit_i])?;

            // 트레이 아이콘 생성
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .tooltip("SwiftCast - AI API Proxy")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // 창 닫기 버튼 클릭 시 숨기기 (종료하지 않음)
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
