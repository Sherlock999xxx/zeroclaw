# Social Channels

Broadcast / social-feed integrations. These differ from chat channels in two ways: messages are typically public, and the agent often acts as a poster rather than a bidirectional responder.

> **Build note:** Social channels are **not included** in the lean default build. To use them, build with `--features channels-full` (all channels) or the specific feature flag (e.g. `--features channel-twitter`). Prebuilt binaries do not include these channels by default. See [Channels → Overview](./overview.md) for the full build-options table.

## Bluesky (AT Protocol)

```toml
[channels.bluesky]
enabled = true
handle = "you.bsky.social"
app_password = "xxxx-xxxx-xxxx-xxxx"      # create at bsky.app/settings/app-passwords
```

- **Auth:** Bluesky app-password (not your real password). Create one in settings.
- **Outbound:** 300-character posts; longer responses auto-thread.
- **Protocol:** AT Protocol via the `atrium-api` crate.

## Nostr

```toml
[channels.nostr]
enabled = true
private_key = "..."                       # nsec bech32 or hex
relays = [
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.primal.net",
]
allowed_pubkeys = ["npub1..."]            # empty = deny all, "*" = allow all
```

- **Auth:** raw private key (`nsec` bech32 or hex). Store in the encrypted secrets backend — never in a checked-in config.
- **Inbound:** kind-1 (text), kind-4 (DM, NIP-04), and kind-1059 (gift-wrap, NIP-17).
- **Outbound:** same kinds. Zap handling is experimental.
- **Relays:** the agent connects to all listed relays; use 3–5 for reliability. If `relays` is omitted, ZeroClaw connects to a built-in set of popular public relays.

## Twitter / X

```toml
[channels.twitter]
enabled = true
bearer_token = "..."                      # Twitter API v2 OAuth 2.0 Bearer Token
allowed_users = ["singlerider"]           # usernames or user IDs; empty = deny all, "*" = allow all
```

- **Auth:** Twitter API v2 OAuth 2.0 Bearer Token only.
- **Inbound:** mentions via the Filtered Stream endpoint.
- **Outbound:** posts, replies, threads.
- **Caveat:** the free tier is rate-limited to the point of near-uselessness. Budget accordingly.

## Reddit

```toml
[channels.reddit]
enabled = true
client_id = "..."
client_secret = "..."
refresh_token = "..."                     # OAuth 2.0 refresh token (required)
username = "your-bot-username"            # without `u/` prefix
subreddit = "rust"                        # optional: filter to a single subreddit (without `r/` prefix)
```

- **Auth:** OAuth 2.0 with a refresh token. Generate one with a script-type Reddit app and the `password` or `code` flow, then save the refresh token here for persistent access.
- **Inbound:** new posts and comments in the configured subreddit (or all subreddits the bot has access to when `subreddit` is unset), plus replies to the agent's own posts.
- **Outbound:** posts, comments, private messages.

## Mastodon

```toml
[channels.mastodon.default]
enabled = true
instance_url = "https://mastodon.social"
access_token = "..."                       # Settings → Development → New Application (read, write:statuses, read:notifications)
allowed_users = ["alice@mastodon.social"]  # user@instance; empty = deny all, "*" = allow all
mention_only = true                        # only respond to statuses that @-mention the bot (DMs always count)
visibility = "direct"                      # direct | private | unlisted | public — outbound reply visibility
poll_interval_secs = 60                    # notification poll cadence
```

- **Auth:** a personal access token from the instance's Development settings. Needs `read`, `write:statuses`, and `read:notifications` scopes.
- **Inbound:** ActivityPub notifications — mentions and direct messages polled every `poll_interval_secs`.
- **Outbound:** status replies posted at the configured `visibility` (defaults to `direct` so replies are not broadcast to public timelines).
- **Slot:** alias-keyed `[channels.mastodon.<alias>]` (any ActivityPub-compatible instance).

## Lemmy

```toml
[channels.lemmy.default]
enabled = true
instance_url = "https://lemmy.world"
username = "your-bot"                       # required when `jwt` is empty
password = "..."                            # required when `jwt` is empty (prefer jwt in production)
jwt = ""                                    # pre-minted JWT; takes precedence over username/password, required for 2FA accounts
allowed_users = ["alice", "bob@lemmy.world"]  # bare or instance-qualified; empty = deny all, "*" = allow all
poll_interval_secs = 30                     # private-message poll cadence (min 5)
```

- **Auth:** username + password auto-login at startup, or a pre-minted `jwt` (recommended for production, required for 2FA-enabled accounts).
- **Inbound:** private messages via `GET /api/v3/private_message/list`, polled every `poll_interval_secs`.
- **Outbound:** private-message replies.
- **Slot:** alias-keyed `[channels.lemmy.<alias>]`.

---

## Operating social channels safely

Bots on public social networks attract adversarial input. Two precautions:

1. **Restrict who the agent will respond to.** Use `allowed_pubkeys` (Nostr) or `allowed_users` (Twitter) to whitelist senders. Bluesky has no per-channel allowlist field — gate at the autonomy / tool layer instead. The default empty-list behaviour is **deny all** for the channels that have an allowlist field.
2. **Keep autonomy level at `Supervised` or lower.** A public-facing agent in `Full` autonomy is effectively a public shell. For public-facing channels, restrict the tool surface in the global tool-policy config rather than expecting per-channel `tools_allow` (no such per-channel field exists).

## Rate limits and backoff

All social channels are subject to aggressive rate limits. ZeroClaw's outbound queue uses exponential backoff on 429 responses. If you hit persistent rate-limiting, throttle the agent's posting cadence at the source rather than relying on per-channel streaming knobs (none of these channels expose draft-update intervals; their schema is intentionally minimal).
