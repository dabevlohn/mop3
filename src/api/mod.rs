pub mod mastodon;
pub mod bluesky;

use crate::config::{Config, ApiMode};
use crate::models::Credentials;
use crate::error::AppResult;

/// Абстрактный интерфейс к социальным сетям
pub trait SocialNetworkApi: Send + Sync {
    /// Проверяет учётные данные и получает информацию о пользователе
    fn verify_credentials(&self, cred: &Credentials) -> AppResult<String>;
    
    /// Получает ленту постов
    fn get_timeline(&self, cred: &Credentials, limit: u32, since_id: &str) -> AppResult<Vec<crate::models::Post>>;
    
    /// Отправляет новый пост
    fn post_status(
        &self,
        cred: &Credentials,
        status: String,
        in_reply_to_id: Option<String>,
        media_ids: Vec<String>,
    ) -> AppResult<String>;
    
    /// Загружает медиа файл
    fn upload_media(&self, cred: &Credentials, data: Vec<u8>, filename: String, mime: String) -> AppResult<String>;
}

/// Фабрика для создания API клиента на основе конфигурации
pub fn create_api_client(config: &Config) -> AppResult<Box<dyn SocialNetworkApi>> {
    match config.api_mode {
        ApiMode::Mastodon => Ok(Box::new(mastodon::MastodonClient::new(config.clone()))),
        ApiMode::Bluesky => Ok(Box::new(bluesky::BlueskyClient::new(config.clone()))),
    }
}
