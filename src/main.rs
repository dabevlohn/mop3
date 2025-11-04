use clap::Parser;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod api;
mod config;
mod error;
mod models;
mod pop3;
mod smtp;

use config::Config;
use error::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    // Инициализируем логирование
    init_tracing()?;

    // Парсим конфигурацию из CLI и env
    let config = Config::parse();

    // Валидируем конфигурацию
    config.validate()?;

    info!(
        "Starting MOP3 gateway - API Mode: {:?}, Listen: {}:{}",
        config.api_mode, config.address, config.pop3port
    );

    // Делим работу на два отдельных потока
    let config_pop3 = Arc::new(config.clone());
    let config_smtp = Arc::new(config.clone());

    // Запускаем POP3 сервер
    let pop3_handle: JoinHandle<AppResult<()>> = {
        let cfg = Arc::clone(&config_pop3);
        tokio::spawn(async move { pop3::server::run_pop3_server(cfg).await })
    };

    // Запускаем SMTP сервер (если не отключен)
    let smtp_handle: Option<JoinHandle<AppResult<()>>> = if config.nosmtp {
        warn!("SMTP server disabled via --nosmtp flag");
        None
    } else {
        Some({
            let cfg = Arc::clone(&config_smtp);
            tokio::spawn(async move { smtp::server::run_smtp_server(cfg).await })
        })
    };

    // Ждём завершения обоих серверов (они работают в бесконечном цикле)
    tokio::select! {
        res = pop3_handle => {
            error!("POP3 server terminated: {:?}", res);
            Err("POP3 server error".into())
        }
        res = async {
            match smtp_handle {
                Some(handle) => handle.await,
                None => std::future::pending().await,
            }
        } => {
            error!("SMTP server terminated: {:?}", res);
            Err("SMTP server error".into())
        }
    }
}

/// Инициализирует систему логирования с использованием tracing
fn init_tracing() -> AppResult<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true)
        .init();

    info!("Tracing initialized");
    Ok(())
}
