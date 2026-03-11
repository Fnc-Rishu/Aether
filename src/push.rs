//! Push subscription orchestration.
//!
//! Coordinates the multi-step registration flow:
//! 1. GCM device check-in → android_id + security_token
//! 2. GCM token registration → gcm_token
//! 3. Generate ECDH P-256 keypair + auth secret for Web Push encryption
//! 4. Build the FCM endpoint URL

use crate::config::{GcmSession, WebPushKeys};
use crate::error::Result;
use crate::fcm::{checkin, register};
use p256::SecretKey;
use rand::RngCore;
use uuid::Uuid;

const FCM_SEND_PREFIX: &str = "https://fcm.googleapis.com/fcm/send/";

/// Holds the result of a complete push subscription registration.
#[derive(Clone)]
pub struct PushSubscription {
    /// The full FCM push endpoint URL
    pub endpoint: String,
    pub gcm: GcmSession,
    pub keys: WebPushKeys,
}

/// Generates a fresh ECDH P-256 key pair and a random 16-byte auth secret.
pub fn generate_keys() -> WebPushKeys {
    let secret_key = SecretKey::random(&mut rand::rngs::OsRng);
    let public_key = secret_key.public_key();

    // SEC1 uncompressed form (65 bytes)
    let public_key_bytes = public_key.to_sec1_bytes().to_vec();
    // Raw scalar (32 bytes)
    let private_key_bytes = secret_key.to_bytes().to_vec();
    // Random auth secret (16 bytes)
    let mut auth_secret = vec![0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut auth_secret);

    WebPushKeys {
        public_key: public_key_bytes,
        private_key: private_key_bytes,
        auth_secret,
    }
}

/// Performs the full registration flow and returns a complete push subscription.
pub async fn subscribe() -> Result<PushSubscription> {
    let http = reqwest::Client::new();

    // Step 1: GCM device check-in
    tracing::debug!("performing GCM device check-in");
    let gcm_session = checkin::checkin_new(&http).await?;

    // Step 2: Register for a GCM token
    let app_id = format!("wp:receiver.push.com#{}", Uuid::new_v4());
    tracing::debug!(app_id = %app_id, "registering GCM token");
    let gcm_token = register::register_gcm(&gcm_session, &app_id).await?;

    // Step 3: Generate Web Push encryption keys
    let keys = generate_keys();

    // Step 4: Build the FCM push endpoint
    let endpoint = format!("{}{}", FCM_SEND_PREFIX, gcm_token);

    tracing::info!(endpoint = %endpoint, "push subscription complete");

    Ok(PushSubscription {
        endpoint,
        gcm: gcm_session,
        keys,
    })
}