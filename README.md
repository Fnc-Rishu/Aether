# Aether
A CLI tool that receives Twitter/X notifications in real time via Google's Firebase Cloud Messaging (FCM) infrastructure. Stream tweets from users you follow and have tweet notifications enabled for.

## Overview

Aether emulates a Chrome browser's Web Push client to receive Twitter/X push notifications via FCM. It connects to Google's MCS (Mobile Connection Server) at `mtalk.google.com:5228` over a persistent TLS socket, decrypts incoming notifications using ECE (Encrypted Content-Encoding), and forwards the decrypted payloads to a configured webhook endpoint.

You will receive notifications for tweets from users that you **follow** and have **tweet notifications turned on** for on Twitter/X.

### How It Works

```
Twitter/X  ──push──▶  FCM (Google MCS)  ◀──TLS/Protobuf──  Aether  ──HTTP POST──▶  Webhook
```

1. Aether performs a GCM device check-in with Google to obtain an `android_id` and `security_token`.
2. It registers a GCM token and constructs the FCM push endpoint URL.
3. The push endpoint, along with ECDH encryption keys, is registered with Twitter's notification settings API.
4. Aether opens a persistent TLS connection to `mtalk.google.com:5228` using the MCS binary protocol.
5. When Twitter sends a push notification, it arrives as a `DataMessageStanza` protobuf message.
6. Aether decrypts the notification payload and forwards it to your webhook.

### Important Notes

- **Data source**: All notification data is received from Google's FCM MCS server (`mtalk.google.com`). Aether does not access Twitter/X directly for notification data.
- **Minimal API usage**: The Twitter/X API is only called during the initial push subscription registration (`register` command). No API calls are made while listening for notifications.
- **No scraping**: This tool does not perform any web scraping. It uses the standard W3C Push API flow via FCM, the same mechanism Chrome uses to deliver push notifications.

## Requirements

- Rust (edition 2021)
- `protoc` (Protocol Buffers compiler) — required by `prost-build` at compile time
- Twitter/X account credentials (`auth_token` and `ct0` cookies)

### Installing protoc

```sh
# Ubuntu/Debian
sudo apt install -y protobuf-compiler

# macOS
brew install protobuf
```

### Getting `auth_token` and `ct0`

1. Open [x.com](https://x.com) in your web browser and log in.
2. Open Developer Tools (F12) and go to the **Application** (or **Storage**) tab.
3. Under **Cookies** → `https://x.com`, find the values for `auth_token` and `ct0`.

## Installation

```sh
cargo install --path .
```

## Usage

### 1. Initialize configuration

```sh
# Interactive mode
aether init

# Or with arguments
aether init --auth-token YOUR_AUTH_TOKEN --ct0 YOUR_CT0
```

This creates `aether.toml` with your Twitter credentials.

### 2. Register push subscription

```sh
aether register
```

This performs:
1. GCM device check-in (obtains `android_id` + `security_token`)
2. GCM token registration
3. Registers the FCM endpoint with Twitter's push notification API

### 3. Start listening

```sh
WEBHOOK_ENDPOINT=https://your-webhook.example.com/endpoint aether listen
```

The `WEBHOOK_ENDPOINT` environment variable specifies where decrypted notification payloads are sent via HTTP POST.

### Other commands

```sh
# Check current configuration and registration status
aether status

# Remove push subscription
aether unregister
```

### Options

| Flag | Description |
|------|-------------|
| `-c, --config <PATH>` | Configuration file path (default: `aether.toml`) |
| `-v, --verbose` | Enable debug logging |

## Reconnection

Aether implements automatic reconnection with exponential backoff:

- Exponential backoff: 5s × 2^n, capped at 5 minutes
- Automatic GCM re-checkin every 5 failed attempts (refreshes security token)
- Infinite retries with counter reset on successful connection
- Heartbeat ping/ack every 4 minutes to detect stale connections

## Architecture

| Component | Description |
|-----------|-------------|
| `fcm/checkin.rs` | GCM device check-in (protobuf over HTTPS) |
| `fcm/register.rs` | GCM token registration (form POST to `c2dm/register3`) |
| `fcm/mcs.rs` | MCS binary protocol client (TLS socket to `mtalk.google.com:5228`) |
| `push.rs` | Orchestrates the full registration flow |
| `twitter.rs` | Registers push endpoint with Twitter API |
| `listener.rs` | Main listen loop with reconnection, decryption, and webhook forwarding |

## License

MIT
