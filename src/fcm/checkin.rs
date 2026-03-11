use crate::config::GcmSession;
use crate::error::{AetherError, Result};
use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/checkin_proto.rs"));
}

const CHECKIN_URL: &str = "https://android.clients.google.com/checkin";

pub async fn checkin_new(http: &reqwest::Client) -> Result<GcmSession> {
    checkin_request(http, None, None).await
}

pub async fn checkin_existing(
    http: &reqwest::Client,
    session: &GcmSession,
) -> Result<GcmSession> {
    checkin_request(
        http,
        Some(session.android_id),
        Some(session.security_token),
    )
    .await
}

async fn checkin_request(
    http: &reqwest::Client,
    android_id: Option<i64>,
    security_token: Option<u64>,
) -> Result<GcmSession> {
    let request = proto::AndroidCheckinRequest {
        version: Some(3),
        id: android_id,
        security_token,
        user_serial_number: Some(0),
        checkin: proto::AndroidCheckinProto {
            r#type: Some(3), 
            chrome_build: Some(proto::ChromeBuildProto {
                platform: Some(2),  
                channel: Some(1),  
                chrome_version: Some(String::from("63.0.3234.0")),
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let body = request.encode_to_vec();

    let response = http
        .post(CHECKIN_URL)
        .body(body)
        .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
        .send()
        .await
        .map_err(|e| AetherError::GcmCheckin(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AetherError::GcmCheckin(format!(
            "checkin failed ({}): {}",
            status, text
        )));
    }

    let response_bytes = response
        .bytes()
        .await
        .map_err(|e| AetherError::GcmCheckin(format!("failed to read response: {}", e)))?;

    let response = proto::AndroidCheckinResponse::decode(response_bytes.as_ref())
        .map_err(|e| AetherError::GcmCheckin(format!("protobuf decode failed: {}", e)))?;

    let raw_android_id = response.android_id.ok_or_else(|| {
        AetherError::GcmCheckin("response missing android_id".to_string())
    })?;
    let android_id = i64::try_from(raw_android_id).map_err(|_| {
        AetherError::GcmCheckin("android_id is not a valid i64".to_string())
    })?;
    let security_token = response.security_token.ok_or_else(|| {
        AetherError::GcmCheckin("response missing security_token".to_string())
    })?;

    tracing::info!(android_id, "GCM checkin succeeded");

    Ok(GcmSession {
        android_id,
        security_token,
    })
}
