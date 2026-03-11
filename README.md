<div align="center">

# ⚡ Aether

**Real-time Twitter/X notification receiver powered by Firebase Cloud Messaging**

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](LICENSE)
[![FCM](https://img.shields.io/badge/Firebase-Cloud_Messaging-FFCA28?style=for-the-badge&logo=firebase&logoColor=black)](https://firebase.google.com/docs/cloud-messaging)

<br/>

*Stream push notifications from Twitter/X in real time — the same way Chrome does.*

<br/>

```
⚡ Sub-second latency  •  🔒 End-to-end encrypted  •  🦀 Pure Rust  •  📡 Zero polling
```

</div>

---

## 🌊 How It Works

Aether emulates a Chrome browser's Web Push client, connecting directly to Google's MCS (Mobile Connection Server) over a persistent TLS socket. When Twitter fires a push notification, it arrives instantly as a protobuf-encoded message — no polling, no scraping, no delays.

```
<img width="2771" height="710" alt="Flowchart" src="https://github.com/user-attachments/assets/a3b92b8c-b9d5-4a89-9fd7-4bedda5e7546" />
```

### 📋 Step by Step

```
  ┌─────────────────────────────────────────────────────────────────┐
  │                     REGISTRATION FLOW                           │
  ├─────────────────────────────────────────────────────────────────┤
  │                                                                 │
  │  1️⃣  GCM Check-in ──▶ android.clients.google.com               │
  │     └─ Obtain android_id + security_token (protobuf)           │
  │                                                                 │
  │  2️⃣  GCM Register ──▶ android.clients.google.com/c2dm/register3│
  │     └─ Get FCM push endpoint URL                               │
  │                                                                 │
  │  3️⃣  Generate ECDH P-256 key pair + auth secret                │
  │     └─ For end-to-end encryption (RFC 8291)                    │
  │                                                                 │
  │  4️⃣  Register with Twitter ──▶ x.com/i/api/1.1/notifications   │
  │     └─ Send endpoint + encryption keys                         │
  │                                                                 │
  └─────────────────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────────────────┐
  │                      LISTENING FLOW                             │
  ├─────────────────────────────────────────────────────────────────┤
  │                                                                 │
  │  5️⃣  Open TLS connection ──▶ mtalk.google.com:5228              │
  │     └─ MCS binary protocol (protobuf over raw TLS)             │
  │                                                                 │
  │  6️⃣  Receive DataMessageStanza                                  │
  │     └─ Encrypted notification payload                          │
  │                                                                 │
  │  7️⃣  Decrypt with ECE (aes128gcm / aesgcm)                     │
  │     └─ Using private key + auth secret                         │
  │                                                                 │
  │  8️⃣  Forward JSON ──▶ POST to your webhook                     │
  │     └─ Clean, decrypted notification payload                   │
  │                                                                 │
  └─────────────────────────────────────────────────────────────────┘
```

---

## ✨ Key Features

| Feature | Description |
|:--------|:------------|
| ⚡ **Real-time** | Sub-second notification delivery via persistent MCS connection |
| 🔒 **Encrypted** | End-to-end encryption with ECDH P-256 + ECE (RFC 8291) |
| 📡 **Zero Polling** | No API calls while listening — pure push notifications |
| 🔄 **Auto Reconnect** | Exponential backoff with automatic GCM re-checkin |
| 💓 **Heartbeat** | 4-minute ping/ack cycle keeps the connection alive |
| 🦀 **Pure Rust** | Fast, safe, single binary — no runtime dependencies |
| 🔑 **Minimal API** | Twitter API only called once during registration |
| 🚫 **No Scraping** | Standard W3C Push API flow, same as Chrome |

---

## 📦 Requirements

| Requirement | Details |
|:------------|:--------|
| **Rust** | Edition 2021+ |
| **protoc** | Protocol Buffers compiler (needed at build time by `prost-build`) |
| **Twitter/X** | `auth_token` and `ct0` cookies from a logged-in session |

### Installing `protoc`

```bash
# Ubuntu / Debian
sudo apt install -y protobuf-compiler

# macOS
brew install protobuf

# Arch Linux
sudo pacman -S protobuf

# Or download from GitHub releases:
# https://github.com/protocolbuffers/protobuf/releases
```

### Getting `auth_token` and `ct0`

1. Open [x.com](https://x.com) and log in
2. Open DevTools → **Application** tab → **Cookies** → `https://x.com`
3. Copy the values for `auth_token` and `ct0`

---

## 🚀 Installation

```bash
git clone https://github.com/Fnc-Rishu/Aether.git
cd Aether
cargo install --path .
```

Verify:
```bash
aether --help
```

---

## 📖 Usage

### 1️⃣ Initialize

```bash
# Interactive (prompts for credentials)
aether init

# Or pass directly
aether init --auth-token YOUR_AUTH_TOKEN --ct0 YOUR_CT0
```
> Creates `aether.toml` with your Twitter credentials.

### 2️⃣ Register

```bash
aether register
```

This performs the full registration chain:
```
GCM Check-in → GCM Token → FCM Endpoint → Twitter API
```

### 3️⃣ Listen

```bash
WEBHOOK_ENDPOINT=https://your-server.com/webhook aether listen
```

> Notifications arrive as HTTP POST requests with the decrypted JSON payload.

### 📊 Status & Management

```bash
aether status       # View config & registration info
aether unregister   # Remove push subscription
```

### 🔧 Options

| Flag | Description |
|:-----|:------------|
| `-c, --config <PATH>` | Config file path (default: `aether.toml`) |
| `-v, --verbose` | Enable debug logging |

---

## 🏗️ Architecture

```
src/
├── main.rs              # CLI entry point & command dispatch
├── cli.rs               # Clap argument definitions
├── config.rs            # TOML config & session persistence
├── error.rs             # Error types (thiserror)
├── push.rs              # Registration orchestrator
├── twitter.rs           # Twitter push subscription API
├── listener.rs          # Main listen loop + reconnection + decryption
└── fcm/
    ├── mod.rs           # Module declarations
    ├── checkin.rs       # GCM device check-in (protobuf ↔ HTTPS)
    ├── register.rs      # GCM token registration (c2dm/register3)
    ├── firebase.rs      # Firebase Installations (reserved)
    └── mcs.rs           # MCS binary protocol (TLS ↔ mtalk.google.com)

proto/
├── android_checkin.proto   # Android check-in message definitions
├── checkin.proto           # Check-in request/response
└── mcs.proto               # MCS protocol (login, heartbeat, data messages)
```

### 🔌 Component Flow

```
                    ┌─────────────┐
                    │   main.rs   │
                    │  (CLI/Clap) │
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │   init   │ │ register │ │  listen  │
        │ (config) │ │ (push.rs)│ │(listener)│
        └──────────┘ └────┬─────┘ └────┬─────┘
                          │            │
                ┌─────────┤            │
                │         │            │
                ▼         ▼            ▼
          ┌──────────┐ ┌──────────┐ ┌──────────┐
          │ checkin  │ │ register │ │  mcs.rs  │
          │   .rs    │ │   .rs    │ │ (socket) │
          └──────────┘ └──────────┘ └────┬─────┘
                                         │
                          ┌──────────────┤
                          │              │
                          ▼              ▼
                    ┌──────────┐  ┌──────────┐
                    │ decrypt  │  │ heartbeat│
                    │  (ECE)   │  │ ping/ack │
                    └────┬─────┘  └──────────┘
                         │
                         ▼
                    ┌──────────┐
                    │ webhook  │
                    │  POST    │
                    └──────────┘
```

---

## 🔄 Reconnection Strategy

Aether automatically handles connection drops with an intelligent retry mechanism:

```
Connection lost
     │
     ▼
┌─────────────────────────────────┐
│  Exponential Backoff            │
│  delay = 5s × 2^n (max 5 min)  │
├─────────────────────────────────┤
│  Every 5 failures:              │
│  → GCM re-checkin               │
│  → Refresh security token       │
├─────────────────────────────────┤
│  On success:                    │
│  → Reset retry counter          │
│  → Resume listening             │
└─────────────────────────────────┘
```

| Parameter | Value |
|:----------|:------|
| Base delay | 5 seconds |
| Backoff multiplier | 2× |
| Max delay | 5 minutes |
| GCM re-checkin | Every 5 failures |
| Heartbeat interval | 4 minutes |
| Heartbeat timeout | 30 seconds |

---

## 🔐 Security Notes

- **No scraping** — Uses the standard W3C Push API flow, identical to how Chrome handles push notifications
- **Minimal API surface** — Twitter's API is only contacted once during `register`. Zero API calls during `listen`
- **End-to-end encryption** — Notifications are encrypted using ECDH P-256 key agreement and ECE (Encrypted Content-Encoding)
- **Local keys** — Your private key never leaves your machine. Only the public key is shared with Twitter

---

## 📄 License

MIT — see [LICENSE](LICENSE) for details.

---

<div align="center">

**Built with 🦀 Rust & ⚡ FCM**

[Report Bug](https://github.com/Fnc-Rishu/Aether/issues) · [Request Feature](https://github.com/Fnc-Rishu/Aether/issues)

</div>
