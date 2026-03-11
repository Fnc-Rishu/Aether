//! Firebase Installation API.
//!
//! Twitter's web app uses the Firebase Installations API to obtain a
//! short-lived auth token, which is then used to call the FCM Registrations
//! API. For our simplified flow (emulating Chrome rather than the full
//! Firebase web SDK), we skip this step and go directly from GCM token to
//! the push endpoint. This module is reserved for future use if full
//! Firebase Installation flow becomes required.

// Currently unused — the GCM-only registration path is sufficient for
// Twitter's push notification API (which accepts the raw FCM endpoint).