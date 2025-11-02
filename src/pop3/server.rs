use crate::config::Config;
use crate::error::AppResult;
use crate::models::Credentials;
use crate::api;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, warn, debug};

pub async fn run_pop3_server(config: Arc<Config>) -> AppResult<()> {
    let bind_addr = format!("{}:{}", config.address, config.pop3port);
    
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("POP3 server listening on: {}", bind_addr);

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                debug!("New POP3 connection from: {}", peer_addr);
                let config = Arc::clone(&config);
                
                // Каждое соединение обрабатывается в отдельной задаче
                tokio::spawn(async move {
                    if let Err(e) = handle_pop3_connection(stream, config).await {
                        warn!("POP3 connection error from {}: {}", peer_addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept POP3 connection: {}", e);
            }
        }
    }
}

async fn handle_pop3_connection(
    mut stream: TcpStream,
    config: Arc<Config>,
) -> AppResult<()> {
    stream.write_all(b"+OK MOP3 ready\r\n").await?;

    // Получаем учётные данные
    let cred = get_pop3_login(&mut stream).await?;
    
    // Берём аккаунт и токен из конфига или из логина
    let mut final_cred = cred;
    if let Some(account) = &config.account {
        final_cred.username = account.clone();
    }
    if let Some(token) = &config.token {
        final_cred.password = token.clone();
    }

    debug!("POP3 login successful for user: {}", final_cred.username);

    // Создаём API клиент
    let api_client = api::create_api_client(&config)?;

    // Проверяем учётные данные
    match api_client.verify_credentials(&final_cred) {
        Ok(account_addr) => {
            stream.write_all(b"+OK MOP3 READY, MESSAGES FETCHED\r\n").await?;
            debug!("Verified account: {}", account_addr);

            // TODO: Получить ленту и отправить письма
            // Пока просто ждём команд от клиента
            handle_pop3_commands(&mut stream).await?;
        }
        Err(e) => {
            error!("Failed to verify credentials: {}", e);
            stream.write_all(b"-ERR Invalid credentials\r\n").await?;
        }
    }

    Ok(())
}

async fn get_pop3_login(stream: &mut TcpStream) -> AppResult<Credentials> {
    let mut cred = Credentials {
        username: String::new(),
        password: String::new(),
    };

    loop {
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await?;
        
        if n == 0 {
            return Err("Connection closed".into());
        }

        let command = String::from_utf8_lossy(&buf[..n]);
        let mut parts = command.split_whitespace();

        match parts.next() {
            Some("USER") => {
                if let Some(username) = parts.next() {
                    cred.username = username.to_string();
                    stream.write_all(b"+OK send PASS\r\n").await?;
                }
            }
            Some("PASS") => {
                if let Some(password) = parts.next() {
                    cred.password = password.to_string();
                    if !cred.username.is_empty() && !cred.password.is_empty() {
                        return Ok(cred);
                    }
                }
            }
            Some("QUIT") => {
                stream.write_all(b"+OK bye\r\n").await?;
                return Err("User quit".into());
            }
            _ => {
                stream.write_all(b"-ERR unknown command\r\n").await?;
            }
        }
    }
}

async fn handle_pop3_commands(stream: &mut TcpStream) -> AppResult<()> {
    let mut buf = vec![0u8; 1024];

    loop {
        let n = stream.read(&mut buf).await?;
        
        if n == 0 {
            break;
        }

        let command = String::from_utf8_lossy(&buf[..n]);
        let mut parts = command.split_whitespace();

        match parts.next() {
            Some("STAT") => {
                stream.write_all(b"+OK 0 0\r\n").await?;
            }
            Some("LIST") => {
                stream.write_all(b"+OK\r\n.\r\n").await?;
            }
            Some("QUIT") => {
                stream.write_all(b"+OK bye\r\n").await?;
                break;
            }
            Some("CAPA") => {
                stream.write_all(b"+OK Capability list follows\r\nUSER\r\nTOP\r\nUIDL\r\n.\r\n").await?;
            }
            Some("NOOP") => {
                stream.write_all(b"+OK\r\n").await?;
            }
            _ => {
                stream.write_all(b"-ERR unknown command\r\n").await?;
            }
        }
    }

    Ok(())
}
