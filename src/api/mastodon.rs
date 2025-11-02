use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::models::{Credentials, MastodonAccount, MastodonStatus, Post};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, error, info, warn};

const USER_AGENT: &str = "mop3/0.2";
const TIMEOUT_SECS: u64 = 30;

pub struct MastodonClient {
    http_client: Client,
    config: Config,
}

impl MastodonClient {
    pub fn new(config: Config) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new());

        MastodonClient {
            http_client,
            config,
        }
    }

    /// Извлекает домен и URL инстанции из username
    fn parse_account(username: &str) -> AppResult<(String, String)> {
        let domain = username
            .rsplit_once('@')
            .map(|parts| parts.1)
            .unwrap_or(username)
            .to_owned();

        let url = if domain.starts_with("https://") {
            domain.clone()
        } else {
            format!("https://{}", domain)
        };

        Ok((domain, url))
    }

    fn get_auth_header(token: &str) -> String {
        format!("Bearer {}", token)
    }
}

impl super::SocialNetworkApi for MastodonClient {
    fn verify_credentials(&self, cred: &Credentials) -> AppResult<String> {
        let (domain, url) = Self::parse_account(&cred.username)?;

        debug!("Verifying Mastodon credentials for domain: {}", domain);

        let response = self
            .http_client
            .get(format!("{}/api/v1/accounts/verify_credentials", url))
            .header("Authorization", Self::get_auth_header(&cred.password))
            .send()
            .map_err(|e| {
                error!("Failed to verify credentials: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::NetworkError(e)
                }
            })?;

        if !response.status().is_success() {
            error!(
                "Invalid credentials for Mastodon account: {}",
                cred.username
            );
            return Err(AppError::InvalidCredentials);
        }

        let account: MastodonAccount = response.json().map_err(|e| {
            error!("Failed to parse account data: {}", e);
            AppError::ApiError("Cannot parse account".to_string())
        })?;

        info!(
            "Successfully verified Mastodon account: {}",
            account.username
        );
        Ok(format!("{}@{}", account.username, domain))
    }

    fn get_timeline(&self, cred: &Credentials, limit: u32, since_id: &str) -> AppResult<Vec<Post>> {
        let (_, url) = Self::parse_account(&cred.username)?;
        let since_query = if !since_id.is_empty() {
            format!("&since_id={}", since_id)
        } else {
            String::new()
        };

        let endpoint = format!(
            "{}/api/v1/timelines/home?limit={}{}",
            url, limit, since_query
        );

        debug!("Fetching Mastodon timeline from: {}", endpoint);

        let response = self
            .http_client
            .get(&endpoint)
            .header("Authorization", Self::get_auth_header(&cred.password))
            .send()
            .map_err(|e| {
                error!("Failed to fetch timeline: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::NetworkError(e)
                }
            })?;

        if !response.status().is_success() {
            error!("API returned status: {}", response.status());
            return Err(AppError::ApiError("Failed to fetch timeline".to_string()));
        }

        let timeline: Vec<MastodonStatus> = response.json().map_err(|e| {
            error!("Failed to parse timeline JSON: {}", e);
            AppError::JsonError(e)
        })?;

        info!("Fetched {} posts from Mastodon timeline", timeline.len());

        let posts = timeline
            .into_iter()
            .map(|status| Post::Mastodon(status))
            .collect();

        Ok(posts)
    }

    fn post_status(
        &self,
        cred: &Credentials,
        status: String,
        in_reply_to_id: Option<String>,
        media_ids: Vec<String>,
    ) -> AppResult<String> {
        let (_, url) = Self::parse_account(&cred.username)?;

        debug!("Posting to Mastodon (reply_to: {:?})", in_reply_to_id);

        let mut body = serde_json::json!({
            "status": status,
        });

        if let Some(id) = in_reply_to_id {
            body["in_reply_to_id"] = Value::String(id);
        }

        if !media_ids.is_empty() {
            body["media_ids"] = Value::Array(
                media_ids
                    .iter()
                    .map(|id| Value::String(id.clone()))
                    .collect(),
            );
        }

        let response = self
            .http_client
            .post(format!("{}/api/v1/statuses", url))
            .header("Authorization", Self::get_auth_header(&cred.password))
            .json(&body)
            .send()
            .map_err(|e| {
                error!("Failed to post status: {}", e);
                if e.is_timeout() {
                    AppError::Timeout
                } else {
                    AppError::ApiError(format!("Post failed: {}", e))
                }
            })?;

        if !response.status().is_success() {
            error!("API returned status: {} for post", response.status());
            return Err(AppError::ApiError("Failed to post".to_string()));
        }

        let result: Value = response.json().map_err(|e| {
            error!("Failed to parse post response: {}", e);
            AppError::JsonError(e)
        })?;

        let post_id = result["id"]
            .as_str()
            .ok_or(AppError::ApiError("No ID in response".to_string()))?
            .to_string();

        info!("Successfully posted to Mastodon: {}", post_id);
        Ok(post_id)
    }

    fn upload_media(
        &self,
        cred: &Credentials,
        data: Vec<u8>,
        filename: String,
        mime: String,
    ) -> AppResult<String> {
        let (_, url) = Self::parse_account(&cred.username)?;

        debug!("Uploading media: {} ({})", filename, mime);

        let part = reqwest::multipart::Part::bytes(data)
            .file_name(filename)
            .mime_str(&mime)
            .map_err(|e| AppError::ApiError(format!("Invalid MIME type: {}", e)))?;

        let form = reqwest::multipart::Form::new().part("file", part);

        let response = self
            .http_client
            .post(format!("{}/api/v2/media", url))
            .header("Authorization", Self::get_auth_header(&cred.password))
            .multipart(form)
            .send()
            .map_err(|e| {
                error!("Failed to upload media: {}", e);
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

        let result: Value = response.json().map_err(|e| {
            error!("Failed to parse upload response: {}", e);
            AppError::JsonError(e)
        })?;

        let media_id = result["id"]
            .as_str()
            .ok_or(AppError::ApiError("No media ID in response".to_string()))?
            .to_string();

        info!("Successfully uploaded media: {}", media_id);
        Ok(media_id)
    }
}
