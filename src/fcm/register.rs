//! GCM token registration.
//!
//! After check-in, registers a "web push" app with Google's c2dm/register3
//! endpoint to obtain a GCM token. This token is then combined with the FCM
//! send URL prefix to form the push endpoint that Twitter will use.

use crate::config::GcmSession;
use crate::error::{AetherError, Result};
use std::collections::HashMap;

const REGISTER_URL: &str = "https://android.clients.google.com/c2dm/register3";

/// VAPID server key in URL-safe base64 (the standard Chrome web push key).
const SERVER_KEY: &str =
    "BDOU99-h67HcA6JeFXHbSNMu7e2yNNu3RzoMj8TM4W88jITfq7ZmPvIM1Iv-4_l2LxQcYwhqby2xGpWwzjfAnG4";

/// Registers with GCM and returns the raw GCM token.
pub async fn register_gcm(session: &GcmSession, app_id: &str) -> Result<String> {
    let android_id_str = session.android_id.to_string();
    let auth_header = format!("AidLogin {}:{}", android_id_str, session.security_token);

    let mut params = HashMap::with_capacity(4);
    params.insert("app", "org.chromium.linux");
    params.insert("X-subtype", app_id);
    params.insert("device", &android_id_str);
    params.insert("sender", SERVER_KEY);

    let response = reqwest::Client::new()
        .post(REGISTER_URL)
        .form(&params)
        .header(reqwest::header::AUTHORIZATION, auth_header)
        .send()
        .await
        .map_err(|e| AetherError::GcmRegister(format!("HTTP request failed: {}", e)))?;

    let response_text = response
        .text()
        .await
        .map_err(|e| AetherError::GcmRegister(format!("failed to read response: {}", e)))?;

    tracing::debug!(response = %response_text, "GCM register response");

    // Response format: "token=<value>" or "Error=<reason>"
    if response_text.starts_with("Error=") {
        let reason = response_text.strip_prefix("Error=").unwrap_or("unknown");
        return Err(AetherError::GcmRegister(format!(
            "GCM registration rejected: {}",
            reason
        )));
    }

    match response_text.strip_prefix("token=") {
        Some(token) => {
            tracing::info!("GCM registration succeeded");
            Ok(token.to_string())
        }
        None => Err(AetherError::GcmRegister(format!(
            "malformed response: {}",
            response_text
        ))),
    }
}