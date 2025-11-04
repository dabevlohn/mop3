use crate::api;
use crate::config::Config;
use crate::error::AppResult;
use crate::models::{Credentials, Post};
use chrono::{DateTime, NaiveDateTime, Utc};
use deunicode::deunicode;
use fancy_regex::Regex;
use mail_builder::MessageBuilder;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

const POP3_BANNER: &[u8] = b"+OK MOP3 ready\r\n";
const POP3_OK_MESSAGES_FETCHED: &[u8] = b"+OK MOP3 READY, MESSAGES FETCHED\r\n";

pub async fn run_pop3_server(config: Arc<Config>) -> AppResult<()> {
    let bind_addr = format!("{}:{}", config.address, config.pop3port);

    let listener = TcpListener::bind(&bind_addr).await?;
    info!("POP3 server listening on: {}", bind_addr);

    let recent_id = String::new();

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                debug!("New POP3 connection from: {}", peer_addr);
                let config = Arc::clone(&config);
                let recent = recent_id.clone();

                tokio::spawn(async move {
                    if let Err(e) = handle_pop3_connection(stream, config, recent).await {
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
    _recent_id: String,
) -> AppResult<()> {
    stream.write_all(POP3_BANNER).await?;

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

    // АСИНХРОННО проверяем учётные данные
    match api_client.verify_credentials(&final_cred).await {
        Ok(account_addr) => {
            info!("Verified account: {}", account_addr);

            // Получаем ленту постов
            match api_client.get_timeline(&final_cred, 40, "").await {
                Ok(posts) => {
                    debug!("Fetched {} posts from timeline", posts.len());

                    // Конвертируем посты в письма
                    let emails = convert_posts_to_emails(posts, &account_addr, &config).await?;

                    let post_size: usize = emails.iter().map(|e| e.len()).sum();

                    stream.write_all(POP3_OK_MESSAGES_FETCHED).await?;

                    // Обрабатываем команды от клиента
                    handle_pop3_commands(&mut stream, &emails, &post_size).await?;
                }
                Err(e) => {
                    error!("Failed to get timeline 0: {}", e);
                    stream
                        .write_all(b"-ERR Failed to fetch messages\r\n")
                        .await?;
                }
            }
        }
        Err(e) => {
            error!("Failed to verify credentials: {}", e);
            stream.write_all(b"-ERR Invalid credentials\r\n").await?;
        }
    }

    Ok(())
}

/// Конвертирует посты Mastodon/Bluesky в RFC822 письма
async fn convert_posts_to_emails(
    posts: Vec<Post>,
    account_addr: &str,
    config: &Arc<Config>,
) -> AppResult<Vec<String>> {
    let mut emails = Vec::new();
    //let domain = account_addr.split('@').last().unwrap_or("mastodon.local");

    for post in posts {
        match post {
            Post::Mastodon(mastodon_post) => {
                if let Ok(email) =
                    convert_mastodon_post_to_email(&mastodon_post, account_addr, config).await
                {
                    emails.push(email);
                }
            }
            Post::Bluesky(_bluesky_post) => {
                debug!("Bluesky post conversion not fully implemented yet");
            }
        }
    }

    Ok(emails)
}

/// Конвертирует один пост Mastodon в RFC822 письмо
async fn convert_mastodon_post_to_email(
    post: &crate::models::MastodonStatus,
    account_addr: &str,
    config: &Arc<Config>,
) -> AppResult<String> {
    // Получаем контент
    let mut content = post.content.clone();

    // Удаляем HTML теги если нужно конвертировать в текст
    if !config.html {
        content = html_to_text(&content);
    }

    // Применяем ASCII преобразование если нужно
    if config.ascii {
        content = deunicode(&content);
    }

    // Применяем proxy для ссылок если нужно
    if let Some(proxy) = &config.proxy {
        content = apply_proxy_to_links(&content, proxy);
    }

    // Определяем тему письма
    let subject = if post.reblog.is_some() {
        format!("mop3 Boost from {}", post.account.display_name)
    } else {
        "mop3 Post".to_string()
    };

    // Парсим дату
    let created_at = parse_timestamp(&post.created_at);

    // Создаём сообщение
    let mut message = MessageBuilder::new()
        .from((post.account.display_name.clone(), post.account.acct.clone()))
        .to(account_addr)
        .subject(subject)
        .date(created_at)
        .message_id(format!("{}@{}", post.id, account_addr));

    // Добавляем тело
    if config.html {
        message = message.html_body(&content);
    } else {
        message = message.text_body(&content);
    }

    // Добавляем reply if header если это ответ
    if let Some(reply_id) = &post.in_reply_to_id {
        message = message.in_reply_to(format!("{}@{}", reply_id, account_addr));
    }

    // Обрабатываем медиа вложения
    for attachment in &post.media_attachments {
        let url = attachment.get("url").and_then(|v| v.as_str());
        let preview_url = attachment.get("preview_url").and_then(|v| v.as_str());

        if let Some(preview_url) = preview_url {
            // Загружаем медиа
            if config.attachment || config.inline {
                if let Ok((data, mime)) = download_media(preview_url).await {
                    let filename = preview_url.split('/').next_back().unwrap_or("image.jpg");
                    if config.attachment {
                        message = message.binary_attachment(mime, filename, data);
                    } else if config.inline {
                        message = message.binary_inline(mime, filename, data);
                    }
                }
            }
            // Добавляем ссылку на оригинальный аттачмент
            if let Some(url) = url {
                message = message.text_body(format!("{}\n> Fullsize: {}\n", content, url));
            }
        }
    }

    // Сериализуем в RFC822
    let email_string = message
        .write_to_string()
        .map_err(|e| format!("Failed to build email: {}", e))?;

    Ok(email_string)
}

/// Загружает медиа файл по URL
async fn download_media(url: &str) -> Result<(Vec<u8>, String), reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        error!("Failed to download media: {}", &response.status());
    }

    let mime = response
        .headers()
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let data = response.bytes().await?;
    Ok((data.to_vec(), mime))
}

/// Конвертирует HTML в обычный текст
fn html_to_text(html: &str) -> String {
    // Простое удаление HTML тегов
    let re = Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(html, "").to_string();

    // Декодируем HTML entities
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<p>", "")
        .replace("https://", "\nhttps://")
        .replace("</p>", "\n")
}

/// Применяет proxy к ссылкам в тексте
fn apply_proxy_to_links(content: &str, proxy: &str) -> String {
    // Найти и заменить HTTP ссылки
    match Regex::new(r"https?://[^\s\]<>]+") {
        Ok(re) => re
            .replace_all(content, |caps: &fancy_regex::Captures| {
                let url = &caps[0];
                format!("{}{}", proxy, url)
            })
            .to_string(),
        Err(_) => content.to_string(),
    }
}

/// Парсит дату Mastodon в Unix timestamp
fn parse_timestamp(date_str: &str) -> i64 {
    if let Ok(dt) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S%.3fZ") {
        DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp()
    } else {
        0
    }
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

async fn handle_pop3_commands(
    stream: &mut TcpStream,
    emails: &[String],
    post_size: &usize,
) -> AppResult<()> {
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
                let response = format!("+OK {} {}\r\n", emails.len(), post_size);
                stream.write_all(response.as_bytes()).await?;
            }
            Some("LIST") => {
                if let Some(index_str) = parts.next() {
                    if let Ok(index) = index_str.parse::<usize>() {
                        if index > 0 && index <= emails.len() {
                            let response = format!("+OK {} {}\r\n", index, emails[index - 1].len());
                            stream.write_all(response.as_bytes()).await?;
                        } else {
                            stream.write_all(b"-ERR no such message\r\n").await?;
                        }
                    } else {
                        stream.write_all(b"-ERR invalid message number\r\n").await?;
                    }
                } else {
                    // LIST без параметра - выводим список всех
                    stream
                        .write_all(format!("+OK {} messages\r\n", emails.len()).as_bytes())
                        .await?;
                    for (i, email) in emails.iter().enumerate() {
                        stream
                            .write_all(format!("{} {}\r\n", i + 1, email.len()).as_bytes())
                            .await?;
                    }
                    stream.write_all(b".\r\n").await?;
                }
            }
            Some("RETR") => {
                if let Some(index_str) = parts.next() {
                    if let Ok(index) = index_str.parse::<usize>() {
                        if index > 0 && index <= emails.len() {
                            let email = &emails[index - 1];
                            stream
                                .write_all(format!("+OK {} octets\r\n", email.len()).as_bytes())
                                .await?;
                            stream.write_all(email.as_bytes()).await?;
                            stream.write_all(b"\r\n.\r\n").await?;
                        } else {
                            stream.write_all(b"-ERR no such message\r\n").await?;
                        }
                    } else {
                        stream.write_all(b"-ERR invalid message number\r\n").await?;
                    }
                } else {
                    stream.write_all(b"-ERR no message specified\r\n").await?;
                }
            }
            Some("DELE") => {
                // Мы не удаляем письма, просто отправляем OK
                stream.write_all(b"+OK\r\n").await?;
            }
            Some("QUIT") => {
                stream.write_all(b"+OK bye\r\n").await?;
                break;
            }
            Some("CAPA") => {
                stream
                    .write_all(b"+OK Capability list follows\r\nUSER\r\nTOP\r\nUIDL\r\n.\r\n")
                    .await?;
            }
            Some("NOOP") => {
                stream.write_all(b"+OK\r\n").await?;
            }
            Some("RSET") => {
                stream.write_all(b"+OK\r\n").await?;
            }
            Some("TOP") => {
                if let (Some(msg_str), Some(lines_str)) = (parts.next(), parts.next()) {
                    if let (Ok(msg), Ok(lines)) =
                        (msg_str.parse::<usize>(), lines_str.parse::<usize>())
                    {
                        if msg > 0 && msg <= emails.len() {
                            let email = &emails[msg - 1];
                            let mut line_count = 0;
                            let mut output = String::new();
                            let mut in_body = false;

                            for line in email.lines() {
                                if line.is_empty() {
                                    in_body = true;
                                }

                                if in_body {
                                    if line_count >= lines {
                                        break;
                                    }
                                    line_count += 1;
                                }

                                output.push_str(line);
                                output.push_str("\r\n");
                            }

                            stream
                                .write_all(format!("+OK {} octets\r\n", output.len()).as_bytes())
                                .await?;
                            stream.write_all(output.as_bytes()).await?;
                            stream.write_all(b".\r\n").await?;
                        } else {
                            stream.write_all(b"-ERR no such message\r\n").await?;
                        }
                    } else {
                        stream.write_all(b"-ERR invalid parameters\r\n").await?;
                    }
                } else {
                    stream.write_all(b"-ERR missing parameters\r\n").await?;
                }
            }
            Some("UIDL") => {
                if let Some(index_str) = parts.next() {
                    if let Ok(index) = index_str.parse::<usize>() {
                        if index > 0 && index <= emails.len() {
                            stream
                                .write_all(format!("+OK {} msg-{}\r\n", index, index).as_bytes())
                                .await?;
                        } else {
                            stream.write_all(b"-ERR no such message\r\n").await?;
                        }
                    } else {
                        stream.write_all(b"-ERR invalid message number\r\n").await?;
                    }
                } else {
                    // UIDL без параметра - выводим список всех
                    stream.write_all(b"+OK\r\n").await?;
                    for i in 1..=emails.len() {
                        stream
                            .write_all(format!("{} msg-{}\r\n", i, i).as_bytes())
                            .await?;
                    }
                    stream.write_all(b".\r\n").await?;
                }
            }
            _ => {
                stream.write_all(b"-ERR unknown command\r\n").await?;
            }
        }
    }

    Ok(())
}
