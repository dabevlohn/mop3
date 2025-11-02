use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum)]
pub enum ApiMode {
    #[value(name = "mastodon")]
    Mastodon,
    #[value(name = "bluesky")]
    Bluesky,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "MOP3")]
#[command(author = "Dabe Vlohn")]
#[command(version = "0.2.0")]
#[command(about = "Mastodon/Bluesky to POP3/SMTP gateway")]
pub struct Config {
    /// Mastodon/Bluesky аккаунт (user@example.com)
    /// Также задаётся через env: MOP3_ACCOUNT
    #[arg(long, env = "MOP3_ACCOUNT")]
    pub account: Option<String>,

    /// Токен авторизации для API
    /// Задаётся через env: MOP3_TOKEN (ОБЯЗАТЕЛЕН для SMTP)
    #[arg(long, env = "MOP3_TOKEN")]
    pub token: Option<String>,

    /// IP адрес для прослушивания
    /// По умолчанию: 127.0.0.1
    /// env: MOP3_ADDRESS
    #[arg(long, env = "MOP3_ADDRESS", default_value = "127.0.0.1")]
    pub address: String,

    /// POP3 порт (по умолчанию: 110)
    /// env: MOP3_POP3_PORT
    #[arg(long, env = "MOP3_POP3_PORT", default_value = "110")]
    pub pop3port: u16,

    /// SMTP порт (по умолчанию: 25)
    /// env: MOP3_SMTP_PORT
    #[arg(long, env = "MOP3_SMTP_PORT", default_value = "25")]
    pub smtp_port: u16,

    /// Режим API: mastodon или bluesky
    /// env: MOP3_API_MODE
    #[arg(long, env = "MOP3_API_MODE", value_enum, default_value = "mastodon")]
    pub api_mode: ApiMode,

    /// Отключить SMTP сервер
    #[arg(long, env = "MOP3_NO_SMTP")]
    pub nosmtp: bool,

    /// Преобразовывать Unicode в ASCII
    #[arg(long, env = "MOP3_ASCII")]
    pub ascii: bool,

    /// Добавлять изображения как вложения
    #[arg(long, env = "MOP3_ATTACHMENT")]
    pub attachment: bool,

    /// Встраивать изображения inline
    #[arg(long, env = "MOP3_INLINE")]
    pub inline: bool,

    /// Отправлять HTML вместо простого текста
    #[arg(long, env = "MOP3_HTML")]
    pub html: bool,

    /// Debug режим: выводить JSON ответов
    #[arg(long, env = "MOP3_DEBUG")]
    pub debug: bool,

    /// Включать URL оригинального поста в письмо
    #[arg(long, env = "MOP3_URL")]
    pub url: bool,

    /// Прокси для ссылок (например: http://frogfind.com/read.php?a=)
    #[arg(long, env = "MOP3_PROXY")]
    pub proxy: Option<String>,
}

impl Config {
    /// Валидирует конфигурацию при запуске
    pub fn validate(&self) -> crate::error::AppResult<()> {
        if !self.nosmtp && self.token.is_none() {
            return Err("SMTP требует токен. Предоставьте --token или используйте --nosmtp".into());
        }

        if self.attachment && self.inline {
            return Err("Нельзя использовать одновременно --attachment и --inline".into());
        }

        Ok(())
    }
}
