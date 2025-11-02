use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::models::{BlueskyPost, BlueskyProfile, Credentials, Post};
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
}

impl super::SocialNetworkApi for BlueskyClient {
    async fn verify_credentials(&self, cred: &Credentials) -> AppResult<String> {
        debug!("Verifying Bluesky credentials for: {}", cred.username);

        let response = self
            .http_client
            .post(format!(
                "{}/com.atproto.server.createSession",
                BLUESKY_API_URL
            ))
            .json(&serde_json::json!({
                "identifier": &cred.username,
                "password": &cred.password,
            }))
            .send()
            .await;
        // .map_err(|e| {
        //     error!("Failed to verify Bluesky credentials: {}", e);
        //     if e.is_timeout() {
        //         AppError::Timeout
        //     } else {
        //         AppError::InvalidCredentials
        //     }
        // })?;

        if response.is_err() {
            error!("Invalid Bluesky credentials");
            return Err(AppError::InvalidCredentials);
        }

        info!("Successfully verified Bluesky account: {}", cred.username);
        Ok(cred.username.clone())
    }

    fn get_timeline(&self, cred: &Credentials, limit: u32, since_id: &str) -> AppResult<Vec<Post>> {
        debug!("Fetching Bluesky timeline");

        // TODO: реализовать получение timeline для Bluesky
        // Требуется authentication token и call к getTimeline endpoint

        warn!("Bluesky timeline fetching not fully implemented yet");
        Ok(vec![])
    }

    fn post_status(
        &self,
        cred: &Credentials,
        status: String,
        in_reply_to_id: Option<String>,
        media_ids: Vec<String>,
    ) -> AppResult<String> {
        debug!("Posting to Bluesky (reply_to: {:?})", in_reply_to_id);

        // TODO: реализовать отправку поста в Bluesky

        warn!("Bluesky post not fully implemented yet");
        Ok("bluesky_post_id".to_string())
    }

    fn upload_media(
        &self,
        cred: &Credentials,
        data: Vec<u8>,
        filename: String,
        mime: String,
    ) -> AppResult<String> {
        debug!("Uploading media to Bluesky: {} ({})", filename, mime);

        // TODO: реализовать загрузку медиа в Bluesky

        warn!("Bluesky media upload not fully implemented yet");
        Ok("bluesky_media_id".to_string())
    }
}
