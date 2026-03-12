use thiserror::Error;

#[derive(Error, Debug)]
pub enum AetherError {
    #[error("config error: {0}")]
    Config(String),

    #[error("GCM checkin error: {0}")]
    GcmCheckin(String),

    #[error("GCM registration error: {0}")]
    GcmRegister(String),

    #[error("FCM error: {0}")]
    Fcm(String),

    #[error("MCS error: {0}")]
    Mcs(String),

    #[error("Twitter API error: {0}")]
    TwitterApi(String),

    #[error("decryption error: {0}")]
    Decryption(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}
pub type Result<T> = std::result::Result<T, AetherError>;
