//! MCS (Mobile Connection Server) client.
//!
//! Implements the binary protobuf-framed protocol used by Chrome/Android to
//! maintain a persistent TLS connection to `mtalk.google.com:5228` for
//! receiving FCM push notifications in real time.
//!
//! Protocol:
//! 1. Client opens TLS connection to mtalk.google.com:5228
//! 2. Client sends MCS version byte (41)
//! 3. Client sends LoginRequest (tag=2)
//! 4. Server responds with LoginResponse (tag=3)
//! 5. Bidirectional heartbeat ping/ack and data messages

use crate::config::GcmSession;
use crate::error::{AetherError, Result};
use bytes::{Buf, BytesMut};
use prost::Message;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

/// Protobuf types generated from mcs.proto at build time.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/mcs_proto.rs"));
}

const MCS_HOST: &str = "mtalk.google.com";
const MCS_PORT: u16 = 5228;
const MCS_VERSION: u8 = 41;

/// MCS message tags (from Chromium source).
const TAG_HEARTBEAT_PING: u8 = 0;
const TAG_HEARTBEAT_ACK: u8 = 1;
const TAG_LOGIN_REQUEST: u8 = 2;
const TAG_LOGIN_RESPONSE: u8 = 3;
const TAG_CLOSE: u8 = 4;
const TAG_IQ_STANZA: u8 = 7;
const TAG_DATA_MESSAGE_STANZA: u8 = 8;

/// Heartbeat interval in seconds. Chrome uses ~4 minutes.
const HEARTBEAT_INTERVAL_SECS: u64 = 4 * 60;

/// Heartbeat ack timeout in seconds.
const HEARTBEAT_TIMEOUT_SECS: u64 = 30;

/// A decoded push notification from FCM.
#[derive(Debug, Clone)]
pub struct FcmNotification {
    pub persistent_id: Option<String>,
    pub data: Vec<u8>,
    pub crypto_key: Option<String>,
    pub encryption: Option<String>,
}

/// Active MCS connection.
pub struct McsClient {
    stream: TlsStream<TcpStream>,
    buf: BytesMut,
    /// persistent_ids received in this session, sent on reconnect
    received_persistent_ids: Vec<String>,
}

/// Connects to MCS and performs the login handshake.
pub async fn connect(
    session: &GcmSession,
    received_persistent_ids: Vec<String>,
) -> Result<McsClient> {
    let root_store = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };

    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(tls_config));

    let addr = format!("{}:{}", MCS_HOST, MCS_PORT);
    let tcp_stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| AetherError::Mcs(format!("TCP connect to {} failed: {}", addr, e)))?;

    tcp_stream.set_nodelay(true)?;

    let server_name = rustls::pki_types::ServerName::try_from(MCS_HOST.to_string())
        .map_err(|e| AetherError::Mcs(format!("invalid server name: {}", e)))?;

    let mut stream = connector
        .connect(server_name, tcp_stream)
        .await
        .map_err(|e| AetherError::Mcs(format!("TLS handshake failed: {}", e)))?;

    tracing::debug!("TLS connection established to {}", addr);

    // Send MCS version byte + LoginRequest
    let android_id_str = session.android_id.to_string();
    let login_request = proto::LoginRequest {
        adaptive_heartbeat: Some(false),
        auth_service: Some(2), // ANDROID_ID
        auth_token: session.security_token.to_string(),
        id: "chrome-63.0.3234.0".into(),
        domain: "mcs.android.com".into(),
        device_id: Some(format!("android-{:x}", session.android_id)),
        network_type: Some(1),
        resource: android_id_str.clone(),
        user: android_id_str,
        use_rmq2: Some(true),
        setting: vec![proto::Setting {
            name: "new_vc".into(),
            value: "1".into(),
        }],
        received_persistent_id: received_persistent_ids.clone(),
        ..Default::default()
    };

    let login_bytes = login_request.encode_to_vec();

    // MCS framing: version byte, then tag byte, then varint-encoded length, then payload
    let mut frame = Vec::with_capacity(2 + 10 + login_bytes.len());
    frame.push(MCS_VERSION);
    frame.push(TAG_LOGIN_REQUEST);
    encode_varint(login_bytes.len() as u64, &mut frame);
    frame.extend_from_slice(&login_bytes);

    stream
        .write_all(&frame)
        .await
        .map_err(|e| AetherError::Mcs(format!("failed to send login: {}", e)))?;

    tracing::debug!("LoginRequest sent");

    // Read LoginResponse
    let mut client = McsClient {
        stream,
        buf: BytesMut::with_capacity(4096),
        received_persistent_ids,
    };

    // The first response byte from the server is the MCS version
    let mut version_buf = [0u8; 1];
    client
        .stream
        .read_exact(&mut version_buf)
        .await
        .map_err(|e| AetherError::Mcs(format!("failed to read MCS version: {}", e)))?;

    tracing::debug!(server_mcs_version = version_buf[0], "MCS version received");

    // Read the login response message
    let (tag, data) = client.read_message().await?;
    if tag != TAG_LOGIN_RESPONSE {
        return Err(AetherError::Mcs(format!(
            "expected LoginResponse (tag=3), got tag={}",
            tag
        )));
    }

    let login_response = proto::LoginResponse::decode(data.as_ref())
        .map_err(|e| AetherError::Mcs(format!("failed to decode LoginResponse: {}", e)))?;

    if let Some(ref err) = login_response.error {
        return Err(AetherError::Mcs(format!(
            "login error: code={}, message={:?}",
            err.code,
            err.message
        )));
    }

    tracing::info!(
        id = %login_response.id,
        "MCS login succeeded"
    );

    Ok(client)
}

impl McsClient {
    /// Waits for the next push notification, handling heartbeats automatically.
    ///
    /// Returns `None` if the connection was closed by the server.
    pub async fn next_notification(&mut self) -> Result<Option<FcmNotification>> {
        let mut next_heartbeat = Instant::now() + Duration::from_secs(HEARTBEAT_INTERVAL_SECS);

        loop {
            let remaining = next_heartbeat.saturating_duration_since(Instant::now());

            match tokio::time::timeout(remaining, self.read_message()).await {
                Ok(Ok((tag, data))) => {
                    // Reset heartbeat timer on any received message
                    next_heartbeat =
                        Instant::now() + Duration::from_secs(HEARTBEAT_INTERVAL_SECS);

                    match tag {
                        TAG_DATA_MESSAGE_STANZA => {
                            match self.decode_data_message(&data) {
                                Ok(notification) => return Ok(Some(notification)),
                                Err(e) => {
                                    tracing::warn!(error = %e, "failed to decode data message, skipping");
                                    continue;
                                }
                            }
                        }
                        TAG_HEARTBEAT_PING => {
                            tracing::debug!("HeartbeatPing received, sending HeartbeatAck");
                            self.send_heartbeat_ack().await?;
                        }
                        TAG_HEARTBEAT_ACK => {
                            tracing::debug!("HeartbeatAck received");
                        }
                        TAG_CLOSE => {
                            tracing::info!("server sent Close");
                            return Ok(None);
                        }
                        TAG_IQ_STANZA => {
                            tracing::debug!("IqStanza received (ignored)");
                        }
                        TAG_LOGIN_RESPONSE => {
                            tracing::debug!("duplicate LoginResponse received (ignored)");
                        }
                        other => {
                            tracing::debug!(tag = other, "unknown MCS tag received, ignoring");
                        }
                    }
                }
                Ok(Err(e)) => {
                    return Err(e);
                }
                // Heartbeat timer expired
                Err(_) => {
                    tracing::debug!("sending HeartbeatPing");
                    self.send_heartbeat_ping().await?;

                    // Wait for HeartbeatAck with timeout
                    match tokio::time::timeout(
                        Duration::from_secs(HEARTBEAT_TIMEOUT_SECS),
                        self.read_message(),
                    )
                    .await
                    {
                        Ok(Ok((tag, data))) => {
                            next_heartbeat =
                                Instant::now() + Duration::from_secs(HEARTBEAT_INTERVAL_SECS);
                            match tag {
                                TAG_HEARTBEAT_ACK => {
                                    tracing::debug!("HeartbeatAck received after ping");
                                }
                                TAG_DATA_MESSAGE_STANZA => {
                                    match self.decode_data_message(&data) {
                                        Ok(notification) => return Ok(Some(notification)),
                                        Err(e) => {
                                            tracing::warn!(error = %e, "failed to decode data message");
                                            continue;
                                        }
                                    }
                                }
                                TAG_HEARTBEAT_PING => {
                                    self.send_heartbeat_ack().await?;
                                }
                                TAG_CLOSE => {
                                    tracing::info!("server sent Close during heartbeat wait");
                                    return Ok(None);
                                }
                                _ => {
                                    tracing::debug!(tag, "non-ack message during heartbeat wait");
                                }
                            }
                        }
                        Ok(Err(e)) => return Err(e),
                        Err(_) => {
                            return Err(AetherError::Mcs(
                                "heartbeat ack timed out, reconnection needed".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    /// Returns the list of persistent_ids received in this session.
    #[allow(dead_code)]
    pub fn received_persistent_ids(&self) -> &[String] {
        &self.received_persistent_ids
    }

    fn decode_data_message(&mut self, data: &[u8]) -> Result<FcmNotification> {
        let message = proto::DataMessageStanza::decode(data)
            .map_err(|e| AetherError::Mcs(format!("DataMessageStanza decode: {}", e)))?;

        tracing::info!(
            from = %message.from,
            category = %message.category,
            persistent_id = ?message.persistent_id,
            "DataMessageStanza received"
        );

        if let Some(ref pid) = message.persistent_id {
            self.received_persistent_ids.push(pid.clone());
        }

        let raw_data = message.raw_data.ok_or_else(|| {
            AetherError::Mcs("DataMessageStanza has no raw_data".to_string())
        })?;

        let mut crypto_key = None;
        let mut encryption = None;

        for field in &message.app_data {
            match field.key.as_str() {
                "crypto-key" => crypto_key = Some(field.value.clone()),
                "encryption" => encryption = Some(field.value.clone()),
                _ => {}
            }
        }

        Ok(FcmNotification {
            persistent_id: message.persistent_id,
            data: raw_data,
            crypto_key,
            encryption,
        })
    }

    async fn send_heartbeat_ping(&mut self) -> Result<()> {
        let ping = proto::HeartbeatPing::default();
        self.send_message(TAG_HEARTBEAT_PING, &ping.encode_to_vec())
            .await
    }

    async fn send_heartbeat_ack(&mut self) -> Result<()> {
        let ack = proto::HeartbeatAck::default();
        self.send_message(TAG_HEARTBEAT_ACK, &ack.encode_to_vec())
            .await
    }

    async fn send_message(&mut self, tag: u8, payload: &[u8]) -> Result<()> {
        let mut frame = Vec::with_capacity(1 + 10 + payload.len());
        frame.push(tag);
        encode_varint(payload.len() as u64, &mut frame);
        frame.extend_from_slice(payload);

        self.stream
            .write_all(&frame)
            .await
            .map_err(|e| AetherError::Mcs(format!("write failed: {}", e)))?;

        Ok(())
    }

    /// Reads a single MCS message: tag byte + varint length + payload.
    async fn read_message(&mut self) -> Result<(u8, bytes::Bytes)> {
        // Ensure we have at least the tag byte
        while self.buf.is_empty() {
            self.fill_buf().await?;
        }

        let tag = self.buf[0];
        self.buf.advance(1);

        // Read varint-encoded length
        let length = self.read_varint().await?;

        // Read the full payload
        while self.buf.len() < length {
            self.fill_buf().await?;
        }

        let data = self.buf.split_to(length).freeze();

        Ok((tag, data))
    }

    /// Reads a protobuf varint from the buffer, filling as needed.
    async fn read_varint(&mut self) -> Result<usize> {
        let mut result: usize = 0;
        let mut shift: u32 = 0;

        loop {
            while self.buf.is_empty() {
                self.fill_buf().await?;
            }

            let byte = self.buf[0];
            self.buf.advance(1);

            result |= ((byte & 0x7F) as usize) << shift;

            if byte & 0x80 == 0 {
                return Ok(result);
            }

            shift += 7;
            if shift >= 64 {
                return Err(AetherError::Mcs("varint too long".to_string()));
            }
        }
    }

    async fn fill_buf(&mut self) -> Result<()> {
        let n = self
            .stream
            .read_buf(&mut self.buf)
            .await
            .map_err(|e| AetherError::Mcs(format!("read failed: {}", e)))?;

        if n == 0 {
            return Err(AetherError::Mcs(
                "connection closed by server".to_string(),
            ));
        }

        Ok(())
    }
}

/// Encodes a u64 as a protobuf varint and appends it to `out`.
fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        if value < 0x80 {
            out.push(value as u8);
            return;
        }
        out.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
}