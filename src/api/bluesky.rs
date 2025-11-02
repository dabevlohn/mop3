use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::models::{Credentials, Post};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, error, info, warn};

const USER_AGENT: &str = "mop3/0.2";
const TIMEOUT_SECS: u64 = 30;
const BLUESKY_API_URL: &str = "https://bsky.social/xrpc";

pub struct BlueskyClient {
    http_client: Client,
    config: Config,
}

impl BlueskyClient {
    pub fn new(config: Config) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new());

        BlueskyClient {
            http_client,
            config,
        }
    }

    /// Создаёт сессию и получает access token
    async fn create_session(&self, cred: &Credentials) -> AppResult<String> {
        debug!("Creating Bluesky session for: {}", cred.username);

        let response = self
            .http_client
            .post(format!("{}/com.atproto.server.createSession", BLUESKY_API_URL))
            .json(&serde_json::json!({
                "identifier": &cred.username,
                "password": &cred.password,
            }))
            .send()
            .await
            .map_err(|e| {
                error!("Failed to create Bluesky session: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::NetworkError(e)
                }
            })?;

        if !response.status().is_success() {
            error!("Invalid Bluesky credentials");
            return Err(AppError::InvalidCredentials);
        }

        let session: Value = response.json().await.map_err(|e| {
            error!("Failed to parse session response: {}", e);
            AppError::JsonError(e)
        })?;

        let access_token = session["accessJwt"]
            .as_str()
            .ok_or(AppError::ApiError("No access token in response".to_string()))?
            .to_string();

        Ok(access_token)
    }
}

#[async_trait]
impl super::SocialNetworkApi for BlueskyClient {
    async fn verify_credentials(&self, cred: &Credentials) -> AppResult<String> {
        debug!("Verifying Bluesky credentials for: {}", cred.username);

        // Создаём сессию для проверки учётных данных
        let _token = self.create_session(cred).await?;

        info!("Successfully verified Bluesky account: {}", cred.username);
        Ok(cred.username.clone())
    }

    async fn get_timeline(&self, cred: &Credentials, limit: u32, since_id: &str) -> AppResult<Vec<Post>> {
        debug!("Fetching Bluesky timeline (limit: {})", limit);

        // Получаем access token
        let token = self.create_session(cred).await?;

        // Запрашиваем timeline
        let response = self
            .http_client
            .get(format!("{}/app.bsky.feed.getTimeline", BLUESKY_API_URL))
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("limit", limit.to_string())])
            .send()
            .await
            .map_err(|e| {
                error!("Failed to fetch Bluesky timeline: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::NetworkError(e)
                }
            })?;

        if !response.status().is_success() {
            error!("Bluesky API returned status: {}", response.status());
            return Err(AppError::ApiError("Failed to fetch timeline".to_string()));
        }

        let data: Value = response.json().await.map_err(|e| {
            error!("Failed to parse timeline JSON: {}", e);
            AppError::JsonError(e)
        })?;

        // TODO: Парсить посты в Vec<Post::Bluesky>
        // Пока возвращаем пустой вектор
        warn!("Bluesky timeline parsing not fully implemented yet");
        
        info!("Fetched Bluesky timeline successfully");
        Ok(vec![])
    }

    async fn post_status(
        &self,
        cred: &Credentials,
        status: String,
        in_reply_to_id: Option<String>,
        media_ids: Vec<String>,
    ) -> AppResult<String> {
        debug!("Posting to Bluesky (reply_to: {:?})", in_reply_to_id);

        // Получаем access token
        let token = self.create_session(cred).await?;

        // Создаём запись (post)
        let mut record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": status,
            "createdAt": chrono::Utc::now().to_rfc3339(),
        });

        // Добавляем reply, если есть
        if let Some(reply_to) = in_reply_to_id {
            record["reply"] = serde_json::json!({
                "parent": { "uri": reply_to },
                "root": { "uri": reply_to }
            });
        }

        let response = self
            .http_client
            .post(format!("{}/com.atproto.repo.createRecord", BLUESKY_API_URL))
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "repo": &cred.username,
                "collection": "app.bsky.feed.post",
                "record": record,
            }))
            .send()
            .await
            .map_err(|e| {
                error!("Failed to post to Bluesky: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::ApiError(format!("Post failed: {}", e))
                }
            })?;

        if !response.status().is_success() {
            error!("Bluesky API returned status: {} for post", response.status());
            return Err(AppError::ApiError("Failed to post".to_string()));
        }

        let result: Value = response.json().await.map_err(|e| {
            error!("Failed to parse post response: {}", e);
            AppError::JsonError(e)
        })?;

        let uri = result["uri"]
            .as_str()
            .ok_or(AppError::ApiError("No URI in response".to_string()))?
            .to_string();

        info!("Successfully posted to Bluesky: {}", uri);
        Ok(uri)
    }

    async fn upload_media(&self, cred: &Credentials, data: Vec<u8>, filename: String, mime: String) -> AppResult<String> {
        debug!("Uploading media to Bluesky: {} ({})", filename, mime);

        // Получаем access token
        let token = self.create_session(cred).await?;

        // Загружаем blob
        let response = self
            .http_client
            .post(format!("{}/com.atproto.repo.uploadBlob", BLUESKY_API_URL))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", mime)
            .body(data)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to upload media to Bluesky: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::ApiError(format!("Upload failed: {}", e))
                }
            })?;

        if !response.status().is_success() {
            error!("Media upload returned status: {}", response.status());
            return Err(AppError::ApiError("Upload failed".to_string()));
        }

        let result: Value = response.json().await.map_err(|e| {
            error!("Failed to parse upload response: {}", e);
            AppError::JsonError(e)
        })?;

        let blob_ref = result["blob"]["ref"]["$link"]
            .as_str()
            .ok_or(AppError::ApiError("No blob reference in response".to_string()))?
            .to_string();

        info!("Successfully uploaded media to Bluesky: {}", blob_ref);
        Ok(blob_ref)
    }
}
