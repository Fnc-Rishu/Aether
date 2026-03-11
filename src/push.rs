use crate::config::{GcmSession, WebPushKeys};
use crate::error::Result;
use crate::fcm::{checkin, register};
use p256::SecretKey;
use rand::RngCore;
use uuid::Uuid;

const FCM_SEND_PREFIX: &str = "https://fcm.googleapis.com/fcm/send/";

#[derive(Clone)]
pub struct PushSubscription {
    /// The full FCM push endpoint URL
    pub endpoint: String,
    pub gcm: GcmSession,
    pub keys: WebPushKeys,
}

pub fn generate_keys() -> WebPushKeys {
    let secret_key = SecretKey::random(&mut rand::rngs::OsRng);
    let public_key = secret_key.public_key();

    let public_key_bytes = public_key.to_sec1_bytes().to_vec();

    let private_key_bytes = secret_key.to_bytes().to_vec();

    let mut auth_secret = vec![0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut auth_secret);

    WebPushKeys {
        public_key: public_key_bytes,
        private_key: private_key_bytes,
        auth_secret,
    }
}

pub async fn subscribe(existing_gcm: Option<GcmSession>) -> Result<PushSubscription> {
    let http = reqwest::Client::new();

    let gcm_session = if let Some(session) = existing_gcm {
        tracing::debug!("reusing existing GCM session for new token request");
        session
    } else {
        tracing::debug!("performing fresh GCM device check-in");
        checkin::checkin_new(&http).await?
    };

    let app_id = format!("wp:receiver.push.com#{}", Uuid::new_v4());
    tracing::debug!(app_id = %app_id, "registering GCM token");
    
    let gcm_token = register::register_gcm(&gcm_session, &app_id).await?;
    let keys = generate_keys();
    let endpoint = format!("{}{}", FCM_SEND_PREFIX, gcm_token);

    tracing::info!(endpoint = %endpoint, "push subscription complete");

    Ok(PushSubscription {
        endpoint,
        gcm: gcm_session,
        keys,
    })
}
