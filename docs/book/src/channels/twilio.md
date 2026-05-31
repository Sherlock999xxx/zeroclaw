# Twilio SMS/MMS

ZeroClaw can send and receive SMS and MMS messages via [Twilio](https://www.twilio.com/) Programmable Messaging.

## Configuration

Add a `[channels.twilio.default]` section to your `zeroclaw.toml`:

```toml
[channels.twilio.default]
enabled = true
account_sid = "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
auth_token = "your_auth_token"
from_number = "+15551234567"
webhook_base_url = "https://your-server.com"
```

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `enabled` | Yes | Set to `true` to activate the channel. |
| `account_sid` | Yes | Your Twilio Account SID (starts with `AC`). |
| `auth_token` | Yes | Your Twilio Auth Token. Treated as a secret. |
| `from_number` | Yes | Your Twilio phone number in E.164 format (e.g. `+15551234567`). |
| `webhook_base_url` | No | Public HTTPS URL where Twilio sends webhook events. |
| `proxy_url` | No | HTTP proxy for outbound requests. |
| `mention_patterns` | No | Regex patterns for mention gating. |
| `approval_timeout_secs` | No | Seconds to wait for approval replies (default: 300). |

## Webhook Setup

1. In your Twilio console, go to **Phone Numbers → Active Numbers**.
2. Click your number and set the **A MESSAGE COMES IN** webhook to:
   ```
   https://your-server.com/twilio
   ```
3. Set the HTTP method to **POST**.
4. ZeroClaw verifies the `X-Twilio-Signature` header on every webhook.

## How It Works

- **Inbound**: Twilio sends form-encoded webhooks to `POST /twilio`. ZeroClaw verifies the signature, checks the allowlist, and routes the message to the configured agent.
- **Outbound**: Agent replies are sent back via Twilio's REST API as SMS messages.
- **MMS**: Inbound media attachments are detected from `MediaUrl0..N` fields.

## Allowlist

Control who can message your bot by setting the `external_peers` list:

```toml
[channels.twilio.default]
enabled = true
# ... other fields ...

[agents.default]
channels = ["twilio.default"]
external_peers = ["+15559876543", "+15551112222"]
```

Use `["*"]` to allow all senders.

## Security

- **Signature verification**: All webhooks are verified using HMAC-SHA1 (`X-Twilio-Signature`).
- **Auth token**: Stored as a secret in the config file.
- **Allowlist**: Only authorized phone numbers can trigger the agent.
