use crate::config::Config;
use crate::error::AppResult;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, warn, debug};

pub async fn run_smtp_server(config: Arc<Config>) -> AppResult<()> {
    let bind_addr = format!("{}:{}", config.address, config.smtp_port);
    
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("SMTP server listening on: {}", bind_addr);

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                debug!("New SMTP connection from: {}", peer_addr);
                let config = Arc::clone(&config);
                
                // Каждое соединение обрабатывается в отдельной задаче
                tokio::spawn(async move {
                    if let Err(e) = handle_smtp_connection(stream, config).await {
                        warn!("SMTP connection error from {}: {}", peer_addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept SMTP connection: {}", e);
            }
        }
    }
}

async fn handle_smtp_connection(
    mut stream: TcpStream,
    config: Arc<Config>,
) -> AppResult<()> {
    stream.write_all(b"220 MOP3 SMTP ready\r\n").await?;

    let mut from = String::new();
    let mut buf = vec![0u8; 4096];

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let command = String::from_utf8_lossy(&buf[..n]);
                let mut parts = command.split_whitespace();

                match parts.next() {
                    Some("HELO") => {
                        stream.write_all(b"250 MOP3 ready\r\n").await?;
                    }
                    Some("EHLO") => {
                        stream.write_all(b"250-MOP3\r\n250-SIZE 5000000\r\n250 OK\r\n").await?;
                    }
                    Some("MAIL") => {
                        // MAIL FROM: <user@example.com>
                        if let Some(from_addr) = extract_email_addr(&command) {
                            from = from_addr;
                        }
                        stream.write_all(b"250 OK\r\n").await?;
                    }
                    Some("RCPT") => {
                        stream.write_all(b"250 OK\r\n").await?;
                    }
                    Some("DATA") => {
                        stream.write_all(b"354 Send message\r\n").await?;
                        
                        // TODO: получить email данные и отправить в социальную сеть
                        debug!("Received email from: {}", from);
                        
                        // Читаем данные письма до ".\r\n"
                        let mut email_data = String::new();
                        loop {
                            let mut line_buf = vec![0u8; 1024];
                            match stream.read(&mut line_buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    let line = String::from_utf8_lossy(&line_buf[..n]);
                                    if line.trim() == "." {
                                        break;
                                    }
                                    email_data.push_str(&line);
                                }
                                Err(_) => break,
                            }
                        }
                        
                        stream.write_all(b"250 OK\r\n").await?;
                    }
                    Some("RSET") => {
                        from.clear();
                        stream.write_all(b"250 OK\r\n").await?;
                    }
                    Some("QUIT") => {
                        stream.write_all(b"221 bye\r\n").await?;
                        break;
                    }
                    Some("NOOP") => {
                        stream.write_all(b"250 OK\r\n").await?;
                    }
                    _ => {
                        stream.write_all(b"502 command not implemented\r\n").await?;
                    }
                }
            }
            Err(e) => {
                error!("SMTP read error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn extract_email_addr(command: &str) -> Option<String> {
    // Извлекаем email из MAIL FROM: <user@example.com>
    let start = command.find('<')?;
    let end = command.find('>')?;
    
    if start < end {
        Some(command[start + 1..end].to_string())
    } else {
        None
    }
}
