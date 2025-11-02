// pop3/converter.rs - вспомогательный модуль для конвертации постов в письма

use crate::config::Config;
use crate::error::AppResult;
use crate::models::{MastodonStatus, Post};
use chrono::{DateTime, NaiveDateTime, Utc};
use deunicode::deunicode;
use fancy_regex::Regex;
use mail_builder::MessageBuilder;
//use mail_parser::DateTime;
use std::sync::Arc;
use tracing::{debug, error};

/// Основная функция конвертации постов в письма
pub async fn convert_posts_to_emails(
    posts: Vec<Post>,
    account_addr: &str,
    config: &Arc<Config>,
) -> AppResult<Vec<String>> {
    let mut emails = Vec::new();
    let domain = account_addr.split('@').last().unwrap_or("mastodon.local");

    for post in posts {
        match post {
            Post::Mastodon(mastodon_post) => {
                match convert_mastodon_post_to_email(&mastodon_post, domain, config).await {
                    Ok(email) => {
                        debug!("Converted Mastodon post {} to email", mastodon_post.id);
                        emails.push(email);
                    }
                    Err(e) => {
                        error!(
                            "Failed to convert Mastodon post {}: {}",
                            mastodon_post.id, e
                        );
                    }
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
pub async fn convert_mastodon_post_to_email(
    post: &MastodonStatus,
    domain: &str,
    config: &Arc<Config>,
) -> AppResult<String> {
    // Получаем контент (если это reblog, берем из reblog)
    let (mut content, subject, post_url) = if let Some(reblog) = &post.reblog {
        (
            reblog.content.clone(),
            format!("Boost from {}", reblog.account.display_name),
            reblog.url.clone(),
        )
    } else {
        (post.content.clone(), "Post".to_string(), post.url.clone())
    };

    // Конвертируем HTML в текст если нужно
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

    // Добавляем ссылку на оригинальный пост в конец если нужно
    if config.url {
        content = format!("{}\n\n---\nOriginal: {}", content, post_url);
    }

    // Создаём сообщение
    let account = &post.account;
    let mut message = MessageBuilder::new()
        .from((
            account.display_name.clone(),
            format!("{}@{}", account.username, domain),
        ))
        .to(format!("{}@{}", account.username, domain))
        .subject(&subject)
        .message_id(format!("{}@{}", post.id, domain));

    // Добавляем тело
    if config.html {
        message = message.html_body(content);
    } else {
        message = message.text_body(content);
    }

    // Добавляем reply-to if header если это ответ
    if let Some(reply_id) = &post.in_reply_to_id {
        message = message.in_reply_to(format!("{}@{}", reply_id, domain));
    }

    // Обрабатываем медиа вложения
    let media_attachments = if let Some(reblog) = &post.reblog {
        &reblog.media_attachments
    } else {
        &post.media_attachments
    };

    for attachment in media_attachments {
        let url = attachment.get("url").and_then(|v| v.as_str());

        if let Some(url) = url {
            // Загружаем медиа
            if config.attachment || config.inline {
                match download_media(url).await {
                    Ok(data) => {
                        let media_type = attachment
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("image/jpeg");

                        let filename = attachment
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("image.jpg");

                        if config.attachment {
                            message = message.attachment(media_type, filename, data);
                            debug!("Added attachment: {}", filename);
                        } else if config.inline {
                            message = message.inline(media_type, filename, data);
                            debug!("Added inline image: {}", filename);
                        }
                    }
                    Err(e) => {
                        error!("Failed to download media from {}: {}", url, e);
                    }
                }
            }
        }
    }

    // Сериализуем в RFC822 формат
    let email_string = message
        .write_to_string()
        .map_err(|e| format!("Failed to build email: {}", e))?;

    Ok(email_string)
}

/// Загружает медиа файл по URL асинхронно
pub async fn download_media(url: &str) -> AppResult<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(format!("Failed to download media: HTTP {}", response.status()).into());
    }

    let data = response.bytes().await?;
    Ok(data.to_vec())
}

/// Конвертирует HTML в обычный текст
pub fn html_to_text(html: &str) -> String {
    // Заменяем основные HTML теги на текстовые эквиваленты
    let mut text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n");

    // Удаляем HTML теги используя regex
    if let Ok(re) = Regex::new(r"<[^>]*>") {
        text = re.replace_all(&text, "").to_string();
    }

    // Декодируем HTML entities
    let text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Удаляем лишние пробелы в конце строк
    let text = text
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    // Удаляем множественные пустые строки
    let text = text.replace("\n\n\n", "\n\n");

    text.trim().to_string()
}

/// Применяет proxy к ссылкам в тексте
pub fn apply_proxy_to_links(content: &str, proxy: &str) -> String {
    // Найти и заменить HTTP/HTTPS ссылки
    match Regex::new(r"https?://[^\s\]<>]+") {
        Ok(re) => re
            .replace_all(content, |caps: &fancy_regex::Captures| {
                let url = &caps[0];
                format!("{}{}", proxy, url)
            })
            .to_string(),
        Err(e) => {
            error!("Regex error while applying proxy: {}", e);
            content.to_string()
        }
    }
}

/// Парсит дату Mastodon в Unix timestamp
pub fn parse_timestamp(date_str: &str) -> i64 {
    // Пытаемся распарсить различные форматы дат
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.3fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%dT%H:%M:%S%z",
    ];

    for format in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(date_str, format) {
            return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp();
        }
    }

    // Если не смогли распарсить, возвращаем текущее время
    Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_text() {
        let html = "<p>Hello <b>world</b>!</p>";
        let text = html_to_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("<"));
        assert!(!text.contains(">"));
    }

    #[test]
    fn test_html_entities() {
        let html = "&lt;test&gt; &amp; &quot;quotes&quot;";
        let text = html_to_text(html);
        assert_eq!(text, "<test> & \"quotes\"");
    }

    #[test]
    fn test_apply_proxy_to_links() {
        let content = "Check this: https://example.com/path";
        let proxy = "http://proxy.com/?url=";
        let result = apply_proxy_to_links(content, proxy);
        assert!(result.contains("http://proxy.com/?url=https://example.com/path"));
    }
}
