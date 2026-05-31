//! Twilio SMS/MMS channel — send and receive text and media messages via
//! Twilio's Programmable Messaging API.
//!
//! Webhook-based (push), same pattern as WhatsApp/Telegram/Signal.
//! The `listen()` method is a no-op; inbound messages arrive through the
//! gateway's `POST /twilio` webhook endpoint.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use regex::Regex;
use sha1::Sha1;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;
use zeroclaw_api::channel::{
    Channel, ChannelApprovalRequest, ChannelApprovalResponse, ChannelMessage, SendMessage,
};

type HmacSha1 = Hmac<Sha1>;

/// Module-level pending-approvals map, same pattern as WhatsApp.
/// Twilio uses webhooks, so `request_approval()` (runtime) and the gateway
/// webhook reply intercept may run on different `Arc<TwilioChannel>` instances.
type PendingApprovalsMap = Mutex<HashMap<String, oneshot::Sender<ChannelApprovalResponse>>>;
static PENDING_APPROVALS: LazyLock<Arc<PendingApprovalsMap>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

const TWILIO_API_BASE: &str = "https://api.twilio.com/2010-04-01";

/// Twilio SMS/MMS channel using the Programmable Messaging API.
///
/// Receives inbound messages via the gateway's `POST /twilio` webhook.
/// Sends outbound messages via Twilio's REST API with HTTP Basic Auth.
pub struct TwilioChannel {
    account_sid: String,
    auth_token: String,
    from_number: String,
    /// The alias key under `[channels.twilio.<alias>]` this handle is bound to.
    alias: String,
    /// Resolves inbound external peers from canonical state at message-time.
    peer_resolver: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    /// Per-channel proxy URL override.
    proxy_url: Option<String>,
    /// Compiled mention patterns for mention gating.
    mention_patterns: Vec<Regex>,
    /// Seconds to wait for an operator reply to a `request_approval` prompt.
    approval_timeout_secs: u64,
}

impl TwilioChannel {
    pub fn new(
        account_sid: String,
        auth_token: String,
        from_number: String,
        alias: impl Into<String>,
        peer_resolver: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    ) -> Self {
        Self {
            account_sid,
            auth_token,
            from_number,
            alias: alias.into(),
            peer_resolver,
            proxy_url: None,
            mention_patterns: Vec::new(),
            approval_timeout_secs: 300,
        }
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn account_sid(&self) -> &str {
        &self.account_sid
    }

    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    pub fn from_number(&self) -> &str {
        &self.from_number
    }

    pub fn with_approval_timeout_secs(mut self, secs: u64) -> Self {
        self.approval_timeout_secs = secs;
        self
    }

    pub fn with_proxy_url(mut self, proxy_url: Option<String>) -> Self {
        self.proxy_url = proxy_url;
        self
    }

    pub fn with_mention_patterns(mut self, patterns: Vec<String>) -> Self {
        self.mention_patterns = compile_mention_patterns(&patterns);
        self
    }

    /// Check whether a phone number is in the allowlist.
    pub fn is_number_allowed(&self, number: &str) -> bool {
        let normalized = normalize_phone(number);
        let peers = (self.peer_resolver)();
        if peers.contains(&"*".to_string()) {
            return true;
        }
        peers.iter().any(|p| normalize_phone(p) == normalized)
    }

    /// Verify Twilio webhook signature.
    ///
    /// Twilio computes `HMAC-SHA1(auth_token, url + sorted_params)` where
    /// params are sorted by key and concatenated as `keyvalue` (no separator).
    pub fn verify_signature(
        &self,
        url: &str,
        params: &[(String, String)],
        signature: &str,
    ) -> bool {
        let mut sorted: Vec<&(String, String)> = params.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());

        let mut data = url.to_string();
        for (key, value) in &sorted {
            data.push_str(key);
            data.push_str(value);
        }

        let mut mac = match HmacSha1::new_from_slice(self.auth_token.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(data.as_bytes());
        let bytes = mac.finalize().into_bytes();
        constant_time_eq(&hex::encode(bytes), signature)
    }

    /// Parse an inbound Twilio webhook payload (form-encoded fields already
    /// extracted into a HashMap).
    pub fn parse_inbound(&self, fields: &HashMap<String, String>) -> Option<InboundMessage> {
        let from = fields.get("From")?.clone();
        let body = fields.get("Body").cloned().unwrap_or_default();
        let message_sid = fields
            .get("MessageSid")
            .or_else(|| fields.get("SmsSid"))
            .cloned()
            .unwrap_or_default();

        // Extract media URLs (MediaUrl0, MediaUrl1, ...)
        let mut media_urls: Vec<String> = Vec::new();
        let mut i = 0;
        while let Some(url) = fields.get(&format!("MediaUrl{i}")) {
            media_urls.push(url.clone());
            i += 1;
        }

        Some(InboundMessage {
            from,
            body,
            message_sid,
            media_urls,
            num_media: fields
                .get("NumMedia")
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0),
        })
    }

    fn http_client(&self) -> reqwest::Client {
        let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30));
        if let Some(ref proxy) = self.proxy_url {
            if let Ok(proxy) = reqwest::Proxy::all(proxy) {
                builder = builder.proxy(proxy);
            }
        }
        builder.build().unwrap_or_else(|_| reqwest::Client::new())
    }

    fn send_url(&self) -> String {
        format!(
            "{TWILIO_API_BASE}/Accounts/{}/Messages.json",
            self.account_sid
        )
    }
}

/// Parsed inbound message from a Twilio webhook.
#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub from: String,
    pub body: String,
    pub message_sid: String,
    pub media_urls: Vec<String>,
    pub num_media: u32,
}

// ---------------------------------------------------------------------------
// Channel trait
// ---------------------------------------------------------------------------

#[async_trait]
impl Channel for TwilioChannel {
    fn name(&self) -> &str {
        "twilio"
    }

    async fn send(&self, message: &SendMessage) -> anyhow::Result<()> {
        let client = self.http_client();

        let form: Vec<(String, String)> = vec![
            ("From".into(), self.from_number.clone()),
            ("To".into(), message.recipient.clone()),
            ("Body".into(), message.content.clone()),
        ];

        let response = client
            .post(&self.send_url())
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Twilio send failed ({status}): {body}");
        }

        Ok(())
    }

    /// Twilio is webhook-based; `listen()` is a no-op.
    async fn listen(&self, _tx: tokio::sync::mpsc::Sender<ChannelMessage>) -> anyhow::Result<()> {
        // Webhook mode — inbound messages arrive via the gateway's
        // `POST /twilio` endpoint, not through this method.
        std::future::pending::<()>().await;
        Ok(())
    }

    async fn health_check(&self) -> bool {
        let client = self.http_client();
        let url = format!("{TWILIO_API_BASE}/Accounts/{}.json", self.account_sid);
        client
            .get(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }

    async fn request_approval(
        &self,
        _recipient: &str,
        _request: &ChannelApprovalRequest,
    ) -> anyhow::Result<Option<ChannelApprovalResponse>> {
        let (tx, rx) = oneshot::channel();
        let id = Uuid::new_v4().to_string();
        PENDING_APPROVALS.lock().await.insert(id.clone(), tx);

        let timeout = tokio::time::Duration::from_secs(self.approval_timeout_secs);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => Ok(Some(response)),
            _ => {
                PENDING_APPROVALS.lock().await.remove(&id);
                Ok(None)
            }
        }
    }

    fn self_handle(&self) -> Option<String> {
        Some(self.from_number.clone())
    }
}

impl ::zeroclaw_api::attribution::Attributable for TwilioChannel {
    fn role(&self) -> ::zeroclaw_api::attribution::Role {
        ::zeroclaw_api::attribution::Role::Channel(::zeroclaw_api::attribution::ChannelKind::Twilio)
    }
    fn alias(&self) -> &str {
        &self.alias
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalize a phone number to a comparable form (strip whitespace/dashes,
/// ensure leading `+`).
fn normalize_phone(number: &str) -> String {
    let digits: String = number
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();
    if digits.starts_with('+') {
        digits
    } else {
        format!("+{digits}")
    }
}

fn compile_mention_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

/// Constant-time comparison to avoid timing attacks on signatures.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_channel() -> TwilioChannel {
        TwilioChannel::new(
            "ACtest123".into(),
            "auth_token_secret".into(),
            "+15551234567".into(),
            "default",
            Arc::new(|| vec!["*".into()]),
        )
    }

    fn make_channel_with_peers(peers: Vec<&str>) -> TwilioChannel {
        let peers: Vec<String> = peers.iter().map(|s| (*s).to_string()).collect();
        TwilioChannel::new(
            "ACtest123".into(),
            "auth_token_secret".into(),
            "+15551234567".into(),
            "default",
            Arc::new(move || peers.clone()),
        )
    }

    // ── Channel properties ──────────────────────────────────────────

    #[test]
    fn twilio_channel_name() {
        let ch = make_channel();
        assert_eq!(ch.name(), "twilio");
    }

    #[test]
    fn twilio_alias() {
        let ch = make_channel();
        assert_eq!(ch.alias(), "default");
    }

    #[test]
    fn self_handle_returns_from_number() {
        let ch = make_channel();
        assert_eq!(ch.self_handle(), Some("+15551234567".into()));
    }

    // ── Phone number allowlist ──────────────────────────────────────

    #[test]
    fn number_allowed_exact() {
        let ch = make_channel_with_peers(vec!["+15559876543"]);
        assert!(ch.is_number_allowed("+15559876543"));
        assert!(!ch.is_number_allowed("+15551112222"));
    }

    #[test]
    fn wildcard_allows_anyone() {
        let ch = make_channel();
        assert!(ch.is_number_allowed("+15559876543"));
        assert!(ch.is_number_allowed("+9999999999"));
    }

    #[test]
    fn number_normalization_strips_formatting() {
        let ch = make_channel_with_peers(vec!["+15559876543"]);
        assert!(ch.is_number_allowed("+1 (555) 987-6543"));
        assert!(ch.is_number_allowed("15559876543"));
    }

    // ── Webhook payload parsing ─────────────────────────────────────

    #[test]
    fn parse_inbound_text_message() {
        let ch = make_channel();
        let mut fields = HashMap::new();
        fields.insert("From".into(), "+15559876543".into());
        fields.insert("Body".into(), "Hello, ZeroClaw!".into());
        fields.insert("MessageSid".into(), "SMabc123".into());

        let msg = ch.parse_inbound(&fields).unwrap();
        assert_eq!(msg.from, "+15559876543");
        assert_eq!(msg.body, "Hello, ZeroClaw!");
        assert_eq!(msg.message_sid, "SMabc123");
        assert!(msg.media_urls.is_empty());
        assert_eq!(msg.num_media, 0);
    }

    #[test]
    fn parse_inbound_mms_with_media() {
        let ch = make_channel();
        let mut fields = HashMap::new();
        fields.insert("From".into(), "+15559876543".into());
        fields.insert("Body".into(), "Check this out".into());
        fields.insert("MessageSid".into(), "SMmms456".into());
        fields.insert("NumMedia".into(), "2".into());
        fields.insert("MediaUrl0".into(), "https://example.com/image.jpg".into());
        fields.insert("MediaUrl1".into(), "https://example.com/doc.pdf".into());

        let msg = ch.parse_inbound(&fields).unwrap();
        assert_eq!(msg.media_urls.len(), 2);
        assert_eq!(msg.media_urls[0], "https://example.com/image.jpg");
        assert_eq!(msg.media_urls[1], "https://example.com/doc.pdf");
        assert_eq!(msg.num_media, 2);
    }

    #[test]
    fn parse_inbound_missing_from_returns_none() {
        let ch = make_channel();
        let fields = HashMap::new();
        assert!(ch.parse_inbound(&fields).is_none());
    }

    #[test]
    fn parse_inbound_falls_back_to_sms_sid() {
        let ch = make_channel();
        let mut fields = HashMap::new();
        fields.insert("From".into(), "+15559876543".into());
        fields.insert("Body".into(), "test".into());
        fields.insert("SmsSid".into(), "SMxyz789".into());

        let msg = ch.parse_inbound(&fields).unwrap();
        assert_eq!(msg.message_sid, "SMxyz789");
    }

    // ── Signature verification ──────────────────────────────────────

    #[test]
    fn verify_signature_valid() {
        let ch = make_channel();
        let url = "https://example.com/twilio";
        let params: Vec<(String, String)> = vec![
            ("From".into(), "+15559876543".into()),
            ("Body".into(), "Hello".into()),
            ("MessageSid".into(), "SMabc123".into()),
        ];

        // Compute expected signature the same way Twilio does
        let mut data = url.to_string();
        let mut sorted = params.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in &sorted {
            data.push_str(key);
            data.push_str(value);
        }
        let mut mac = HmacSha1::new_from_slice(ch.auth_token().as_bytes()).unwrap();
        mac.update(data.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());

        assert!(ch.verify_signature(url, &params, &expected));
    }

    #[test]
    fn verify_signature_invalid() {
        let ch = make_channel();
        let params = vec![("From".into(), "+15559876543".into())];
        assert!(!ch.verify_signature("https://example.com/twilio", &params, "badsignature"));
    }

    #[test]
    fn verify_signature_empty_params() {
        let ch = make_channel();
        let url = "https://example.com/twilio";
        let params: Vec<(String, String)> = vec![];

        let mut mac = HmacSha1::new_from_slice(ch.auth_token().as_bytes()).unwrap();
        mac.update(url.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());

        assert!(ch.verify_signature(url, &params, &expected));
    }

    // ── Normalization ───────────────────────────────────────────────

    #[test]
    fn normalize_phone_adds_plus() {
        assert_eq!(normalize_phone("15551234567"), "+15551234567");
    }

    #[test]
    fn normalize_phone_strips_formatting() {
        assert_eq!(normalize_phone("+1 (555) 123-4567"), "+15551234567");
    }

    #[test]
    fn normalize_phone_already_normalized() {
        assert_eq!(normalize_phone("+15551234567"), "+15551234567");
    }

    // ── Constant-time comparison ────────────────────────────────────

    #[test]
    fn constant_time_eq_same() {
        assert!(constant_time_eq("abc", "abc"));
    }

    #[test]
    fn constant_time_eq_different() {
        assert!(!constant_time_eq("abc", "def"));
    }

    #[test]
    fn constant_time_eq_different_length() {
        assert!(!constant_time_eq("abc", "abcd"));
    }

    // ── Send form structure ─────────────────────────────────────────

    #[test]
    fn send_form_contains_required_fields() {
        let ch = make_channel();
        let msg = SendMessage::new("Hello from ZeroClaw!", "+15559876543");

        let form: Vec<(&str, &str)> = vec![
            ("From", ch.from_number()),
            ("To", &msg.recipient),
            ("Body", &msg.content),
        ];

        assert_eq!(form[0].1, "+15551234567");
        assert_eq!(form[1].1, "+15559876543");
        assert_eq!(form[2].1, "Hello from ZeroClaw!");
    }
}
