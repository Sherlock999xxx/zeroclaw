# 8Sleep Integration

> **Disclaimer:** 8Sleep does not publish a stable public API. This integration
> uses the same HTTPS endpoints the official mobile app calls and may break
> without notice.

## Overview

The `eight_sleep` tool lets your agent control an 8Sleep Pod through the cloud
API — read bed state, view sleep metrics, set temperature, toggle priming, and
manage alarms. Each side of the Pod (`left` / `right`) is controlled
independently.

## Setup

### 1. Enable the integration

Add an `[eight_sleep]` section to your `config.toml`:

```toml
[eight_sleep]
enabled = true
email = "you@example.com"
password = "your-8sleep-password"
# device_id = "optional-pod-device-id"  # auto-detected if omitted
# request_timeout_secs = 30
```

### 2. Use environment variables (alternative)

Instead of storing credentials in the config file, you can set:

```bash
export EIGHT_SLEEP_EMAIL="you@example.com"
export EIGHT_SLEEP_PASSWORD="your-8sleep-password"
```

And use a minimal config:

```toml
[eight_sleep]
enabled = true
```

### 3. Verify

When enabled, the `eight_sleep` tool appears in the integrations panel as
**Active** and shows up in `GET /api/tools`.

## Actions

| Action | Type | Required params | Description |
|---|---|---|---|
| `get_bed_state` | Read | — | Current temperature, priming, alarm status |
| `get_metrics` | Read | `side` | Sleep metrics (default: last 24h) |
| `set_temperature` | Act | `side`, `temperature` | Set target temp (-100 to 100) |
| `set_priming` | Act | `side` | Toggle priming on/off |
| `set_alarm` | Act | `side`, `time` | Set alarm time (HH:MM) |

## Example usage

```json
{"action": "get_bed_state"}
```

```json
{"action": "set_temperature", "side": "left", "temperature": 5}
```

```json
{"action": "get_metrics", "side": "right"}
```

```json
{"action": "set_alarm", "side": "left", "time": "07:00", "enabled": true}
```

## Breakage modes

- 8Sleep may change their API without notice, breaking this integration.
- JWT tokens are cached in-memory only — restarting ZeroClaw triggers a fresh
  login.
- If your 8Sleep account has 2FA enabled, this tool will not work (the API
  does not support 2FA flows for third-party integrations).

## Rollback

Set `enabled = false` or remove the `[eight_sleep]` section entirely.
