//! Firebase Installation API.
//!
//! Normally the web app gets a short-lived auth token from Firebase
//! Installations before registering with FCM.
//!
//! Current implementation skips that and registers directly using the
//! GCM/FCM token. This module exists only if full Firebase flow is
//! needed later.

// Currently unused, Twitter accepts the raw FCM endpoint.
