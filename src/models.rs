use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub media_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MastodonAccount {
    pub display_name: String,
    pub username: String,
    pub acct: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlueskyProfile {
    pub display_name: Option<String>,
    pub handle: String,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MastodonStatus {
    pub id: String,
    pub content: String,
    pub created_at: String,
    pub url: String,
    pub reblog: Option<Box<MastodonStatus>>,
    pub in_reply_to_id: Option<String>,
    pub media_attachments: Vec<serde_json::Value>,
    #[serde(default)]
    pub account: MastodonAccount,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlueskyPost {
    pub uri: String,
    pub text: String,
    pub created_at: String,
    pub reply: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum Post {
    Mastodon(MastodonStatus),
    Bluesky(BlueskyPost),
}

#[derive(Debug, Clone)]
pub struct Email {
    pub id: String,
    pub domain: String,
    pub content: String,
    pub subject: String,
}
