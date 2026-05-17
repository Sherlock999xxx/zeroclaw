# Routing

Routing happens at the **agent layer**. Each agent points at exactly one provider; channels point at agents. There is no meta-provider that selects between backends, and no in-process fallback chain.

Two layers of decisions:

1. **Per-call backend selection** — "use the cheap model unless this prompt looks like reasoning." Each routing target is its own `[agents.<alias>]` entry with its own `model_provider`. Channels are routed to whichever agent should handle their traffic.
2. **Provider reliability** — "if Claude times out, automatically use OpenAI." This is OpenRouter's job, not ZeroClaw's. Configure OpenRouter as a normal provider and let it handle vendor fan-out.

## Per-agent dispatch

Define each routing target as its own agent, then point channels at the agent that should handle their traffic.

```toml
[providers.models.anthropic.sonnet]
model   = "claude-sonnet-4-6"
api_key = "sk-ant-..."

[providers.models.anthropic.haiku]
model   = "claude-haiku-4-5-20251001"
api_key = "sk-ant-..."

[providers.models.deepseek.reasoner]
model   = "deepseek-reasoner"
api_key = "sk-..."

[providers.models.gemini.vision]
model   = "gemini-2.5-pro"
api_key = "..."

[channels.telegram.home]
bot_token = "..."

[channels.slack.engineering]
bot_token = "..."

[channels.slack.research]
bot_token = "..."

[channels.discord.media]
bot_token = "..."

[agents.fast]
model_provider  = "anthropic.haiku"
risk_profile    = "hardened"
runtime_profile = "tight"             # snappy public replies
channels        = ["telegram.home"]

[agents.deep]
model_provider  = "anthropic.sonnet"
risk_profile    = "hardened"
runtime_profile = "deep"              # extended engineering tasks
channels        = ["slack.engineering"]

[agents.reasoner]
model_provider  = "deepseek.reasoner"
risk_profile    = "hardened"
runtime_profile = "deep"              # research-style reasoning chains
channels        = ["slack.research"]

[agents.eyes]
model_provider  = "gemini.vision"
risk_profile    = "hardened"
runtime_profile = "tight"             # quick image-bearing replies
channels        = ["discord.media"]

[risk_profiles.hardened]
level                            = "supervised"
workspace_only                   = true
require_approval_for_medium_risk = true
block_high_risk_commands         = true

[runtime_profiles.tight]
max_tool_iterations  = 5
max_actions_per_hour = 30

[runtime_profiles.deep]
max_tool_iterations  = 50
max_actions_per_hour = 200
```

Each channel binds to one agent. Channels can move between agents by editing `channels = [...]` on the agent that should pick them up; `Config::validate()` makes sure references resolve.

For ad-hoc multi-step routing inside a single conversation, use the `delegate` tool: an agent can hand off to another configured agent (referenced by its alias).

## Reliability via OpenRouter

OpenRouter is a single first-class provider. The runtime sees one endpoint; OpenRouter handles vendor fan-out, model selection, and uptime behind that endpoint.

```toml
[providers.models.openrouter.home]
model   = "anthropic/claude-sonnet-4-20250514"
api_key = "sk-or-..."

[agents.assistant]
model_provider = "openrouter.home"
risk_profile   = "hardened"

[risk_profiles.hardened]
level = "supervised"
```

If OpenRouter is unavailable, that's an outage — there is no in-process fallback. Operators who need cross-vendor reliability run multiple ZeroClaw instances behind a load balancer or use OpenRouter's enterprise SLA.

## Why no in-process fallback

1. **Failure modes are vendor-specific.** "Provider returned 500" means different things for different vendors; a single retry-and-fall-through policy hides bugs more often than it catches them.
2. **State across providers is hard.** A fallback chain that swaps providers mid-conversation has to reconcile message-format differences, tool-call IDs, and reasoning-token shapes. Doing it correctly is a lot of code; doing it incorrectly silently corrupts conversation state.
3. **OpenRouter does it better.** Vendor fan-out is OpenRouter's whole product.
4. **Per-agent dispatch is more honest.** When two channels should use different models, naming two agents is clearer than encoding the routing rule inside a meta-provider.

## Hint-based model routes

A separate, narrower mechanism: `[[model_routes]]` lets an agent override the configured `model_provider` for prompts marked with a hint string. Useful when one agent should occasionally reach for a different model without spinning up a second agent.

```toml
[[model_routes]]
hint           = "reasoning"
model_provider = "deepseek"
model          = "deepseek-reasoner"
```

Routes only fire when a prompt explicitly carries the matching hint. The default request path uses the agent's primary `model_provider`.

## Observability

Per-agent dispatch decisions are visible in tracing logs:

```
INFO channel=telegram.home routed to agent=fast
INFO agent=fast model_provider=anthropic.haiku turn_id=...
INFO model_provider=anthropic.haiku stream complete tokens={input=512, output=128}
```

For production deployments, wire the log output to Loki / Grafana. See [Operations → Logs & observability](../ops/observability.md).

## See also

- [Overview](./overview.md) — provider model and per-agent dispatch
- [Configuration](./configuration.md) — full `[providers.*]` schema
- [Provider catalog](./catalog.md) — every canonical slot
