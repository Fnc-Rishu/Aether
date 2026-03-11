use crate::config::{self, Registration};
use crate::error::{AetherError, Result};
use crate::fcm::{checkin, mcs};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;

/// Outcome of a single listen session.
enum SessionOutcome {
    /// Connection closed normally.
    NormalClose,
    /// Error after a successful login (connection was established).
    DisconnectedAfterConnect(AetherError),
    /// Failed before login completed.
    ConnectionFailed(AetherError),
}

/// Computes exponential backoff: 5s × 2^n, capped at 5 minutes.
fn calc_backoff(retry_count: u32) -> u64 {
    const BASE_SECS: u64 = 5;
    const MAX_SECS: u64 = 5 * 60;
    std::cmp::min(
        BASE_SECS.saturating_mul(2u64.saturating_pow(retry_count.saturating_sub(1))),
        MAX_SECS,
    )
}

/// Main listen loop with automatic reconnection.
pub async fn listen(mut registration: Registration, config_path: &Path) -> Result<()> {
    let mut retry_count: u32 = 0;
    let mut persistent_ids: Vec<String> = Vec::new();

    loop {
        match listen_once(&registration, &persistent_ids).await {
            SessionOutcome::NormalClose => {
                retry_count = 0;
                tracing::info!("MCS connection closed, reconnecting");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            SessionOutcome::DisconnectedAfterConnect(e) => {
                retry_count = 0;
                tracing::info!(error = %e, "disconnected after connect, reconnecting");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            SessionOutcome::ConnectionFailed(e) => {
                retry_count += 1;
                let delay = calc_backoff(retry_count);

                // On repeated failures, re-checkin to refresh security token
                if retry_count % 5 == 0 {
                    tracing::warn!("multiple failures, re-checking in to GCM");
                    match refresh_session(&mut registration, config_path).await {
                        Ok(()) => {
                            tracing::info!("GCM re-checkin succeeded");
                        }
                        Err(re) => {
                            tracing::error!(error = %re, "GCM re-checkin failed");
                        }
                    }
                }

                tracing::warn!(
                    retry_count,
                    delay_secs = delay,
                    error = %e,
                    "MCS connection failed, retrying"
                );
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
        }
    }
}

/// Refreshes the GCM session (re-checkin) and persists it.
async fn refresh_session(registration: &mut Registration, config_path: &Path) -> Result<()> {
    let http = reqwest::Client::new();
    let new_session = checkin::checkin_existing(&http, &registration.gcm).await?;
    registration.gcm = new_session;

    let mut full_config = config::Config::load(config_path)?;
    full_config.registration = Some(registration.clone());
    full_config.save(config_path)?;
    Ok(())
}

/// Runs a single listen session.
async fn listen_once(
    registration: &Registration,
    persistent_ids: &[String],
) -> SessionOutcome {
    let mut client = match mcs::connect(&registration.gcm, persistent_ids.to_vec()).await {
        Ok(client) => client,
        Err(e) => return SessionOutcome::ConnectionFailed(e),
    };

    tracing::info!("MCS connection established, listening for notifications");

    match run_notification_loop(&mut client, &registration.keys).await {
        Ok(()) => SessionOutcome::NormalClose,
        Err(e) => SessionOutcome::DisconnectedAfterConnect(e),
    }
}

/// Receives and processes notifications until the connection drops.
async fn run_notification_loop(
    client: &mut mcs::McsClient,
    keys: &config::WebPushKeys,
) -> Result<()> {
    while let Some(notification) = client.next_notification().await? {
        tracing::info!(
            persistent_id = ?notification.persistent_id,
            encrypted_size = notification.data.len(),
            "notification received"
        );

        match handle_notification(&notification, keys).await {
            Ok(()) => {
                tracing::info!("notification processed successfully");
            }
            Err(e) => {
                tracing::warn!(error = %e, "notification processing failed");
            }
        }
    }

    Ok(())
}

async fn handle_notification(
    notification: &mcs::FcmNotification,
    keys: &config::WebPushKeys,
) -> Result<()> {
    let decrypted = decrypt_notification(notification, keys)?;

    let text = String::from_utf8(decrypted)
        .map_err(|e| AetherError::Decryption(format!("UTF-8 conversion failed: {}", e)))?;

    let payload: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "raw": text }));

    tracing::info!(payload = %payload, "notification decrypted");

    send_to_webhook(&payload).await?;

    Ok(())
}

fn decrypt_notification(
    notification: &mcs::FcmNotification,
    keys: &config::WebPushKeys,
) -> Result<Vec<u8>> {
    use base64::engine::general_purpose::URL_SAFE;
    use base64::Engine;

    let key_pair = ece::EcKeyComponents::new(keys.private_key.clone(), keys.public_key.clone());

    // Check if we have aesgcm-style headers (crypto-key + encryption in app_data)
    if let (Some(ref crypto_key_header), Some(ref encryption_header)) =
        (&notification.crypto_key, &notification.encryption)
    {
        let dh_value = extract_param(crypto_key_header, "dh").ok_or_else(|| {
            AetherError::Decryption("missing 'dh' param in crypto-key".to_string())
        })?;
        let salt_value = extract_param(encryption_header, "salt").ok_or_else(|| {
            AetherError::Decryption("missing 'salt' param in encryption".to_string())
        })?;

        let sender_public_key = URL_SAFE.decode(&dh_value).map_err(|e| {
            AetherError::Decryption(format!("base64 decode sender key: {}", e))
        })?;
        let salt = URL_SAFE.decode(&salt_value).map_err(|e| {
            AetherError::Decryption(format!("base64 decode salt: {}", e))
        })?;

        let block = ece::legacy::AesGcmEncryptedBlock::new(
            &sender_public_key,
            &salt,
            4096,
            notification.data.clone(),
        )
        .map_err(|e| AetherError::Decryption(format!("AesGcmEncryptedBlock: {}", e)))?;

        let decrypted = ece::legacy::decrypt_aesgcm(&key_pair, &keys.auth_secret, &block)
            .map_err(|e| AetherError::Decryption(format!("aesgcm decrypt: {}", e)))?;

        tracing::debug!(decrypted_len = decrypted.len(), "aesgcm decryption succeeded");
        return Ok(decrypted);
    }

    // Fallback: try aes128gcm (the data itself contains the header)
    let decrypted = ece::decrypt(&key_pair, &keys.auth_secret, &notification.data)
        .map_err(|e| AetherError::Decryption(format!("aes128gcm decrypt: {}", e)))?;

    tracing::debug!(
        decrypted_len = decrypted.len(),
        "aes128gcm decryption succeeded"
    );
    Ok(decrypted)
}

/// Extracts a named parameter from a semicolon/comma-delimited header value.
///
/// Handles formats like `dh=abc123` and `dh=abc123;p256ecdsa=xyz`.
fn extract_param(header: &str, param: &str) -> Option<String> {
    for part in header.split(|c| c == ';' || c == ',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&format!("{}=", param)) {
            return Some(value.to_string());
        }
    }
    None
}

/// Sends the decrypted notification payload to the configured webhook endpoint via HTTP POST.
async fn send_to_webhook(payload: &serde_json::Value) -> Result<()> {
    let webhook_url = config::get_webhook_endpoint()?;
    let client = Client::new();

    tracing::info!(url = %webhook_url, "sending to webhook");

    let response = client.post(&webhook_url).json(payload).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("failed to read response body"));
        tracing::warn!(status = %status, body = %error_text, "webhook request failed");
    } else {
        tracing::info!(status = %response.status(), "webhook request succeeded");
    }

    Ok(())
}