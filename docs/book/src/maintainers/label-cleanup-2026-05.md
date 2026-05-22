# Label Cleanup Snapshot, May 2026

This page records the maintainer cleanup plan from the May 2026 label audit. It is a snapshot, not live automation. Before running any command, refresh the labels and open issue/PR refs against GitHub.

This cleanup is intentionally split into buckets:

1. Zero-history labels that can be deleted after exact approval.
2. Zero-open duplicate labels where the old spelling only remains on closed historical items.
3. Active legacy labels that must be migrated on open issues/PRs before deleting the old label.
4. Holdbacks that need a policy decision before cleanup.

The audit was read-only. It did not delete labels, relabel issues or PRs, update boards, post comments, or mutate GitHub state.

## Summary

| Metric | Count |
|---|---:|
| Total labels | 400 |
| Open label references | 3927 |
| Historical label references | 22745 |
| Labels with zero open refs | 231 |
| Labels with zero historical refs | 30 |
| Normalized duplicate groups | 87 |
| Bucket A conservative zero-history deletes | 13 |
| Bucket B zero-open duplicate deletes | 62 |
| Bucket C migrate-first legacy labels | 17 |
| Bucket D holdbacks / policy decisions | 33 |

## Cleanup sequence

Use this order:

1. Delete the Bucket A labels only after exact maintainer approval.
2. Decide whether preserving old labels on closed historical items matters. If not, delete Bucket B as one mechanical old-spelling cleanup batch.
3. Migrate Bucket C open refs from legacy spelling to canonical spelling, verify the legacy label has zero open refs, then delete the legacy label.
4. Keep Bucket D out of mechanical cleanup until the policy semantics are settled.

Do not run label deletion, relabeling, issue closure, milestone changes, or board changes from this page alone. Each live mutation batch needs exact approval.

## Bucket A: conservative zero-history deletes

These labels had zero open refs and zero historical refs during the audit.

| Delete label | Keep / replacement | Rationale |
|---|---|---|
| `agent: quota_aware` | top-level/module label | No current refs or policy/doc usage found. |
| `agent: research` | top-level/module label | No current refs or policy/doc usage found. |
| `channel: email` | `channel:email` | Old spaced duplicate with no refs. |
| `channel: mattermost` | `channel:mattermost` | Old spaced duplicate with no refs. |
| `cron:schedule` | `cron` | Stale module fragment with no refs. |
| `cron:types` | `cron` | Stale module fragment with no refs. |
| `health:core` | `health` | Stale core sublabel with no refs. |
| `heartbeat:core` | `heartbeat` | Stale core sublabel with no refs. |
| `provider: quota_cli` | `provider` | Stale module fragment with no refs. |
| `skills: templates` | `skills` | Stale module fragment with no refs. |
| `tool: cron_run` | `tool:cron` | Covered by the grouped cron tool label. |
| `tunnel: cloudflare` | `tunnel:cloudflare` or `tunnel` | Old spaced duplicate with no refs; final replacement depends on tunnel-label policy. |
| `tunnel:core` | `tunnel` | Stale core sublabel with no refs. |

## Bucket B: zero-open duplicate labels

These labels had no open refs and had a canonical replacement spelling. Deleting them cleans up old spellings on closed historical issues/PRs, so confirm that history-display cleanup is acceptable before deleting.

| Delete legacy label | Canonical label | Legacy all refs | Canonical open refs |
|---|---|---:|---:|
| `agent: core` | `agent:core` | 1 | 0 |
| `agent: dispatcher` | `agent:dispatcher` | 5 | 0 |
| `agent: agent` | `agent:agent` | 15 | 0 |
| `agent: loop` | `agent:loop` | 160 | 2 |
| `channel: imessage` | `channel:imessage` | 1 | 3 |
| `channel: mqtt` | `channel:mqtt` | 1 | 1 |
| `channel: linq` | `channel:linq` | 2 | 7 |
| `channel: dingtalk` | `channel:dingtalk` | 4 | 14 |
| `channel: irc` | `channel:irc` | 4 | 22 |
| `channel: slack` | `channel:slack` | 5 | 19 |
| `channel: whatsapp` | `channel:whatsapp` | 8 | 32 |
| `channel: lark` | `channel:lark` | 16 | 15 |
| `channel: core` | `channel:core` | 54 | 1 |
| `channel: telegram` | `channel:telegram` | 113 | 40 |
| `config: core` | `config:core` | 407 | 0 |
| `cron: core` | `cron:core` | 2 | 0 |
| `cron: store` | `cron:store` | 9 | 0 |
| `cron: scheduler` | `cron:scheduler` | 103 | 3 |
| `daemon: core` | `daemon:core` | 139 | 0 |
| `doctor: core` | `doctor:core` | 70 | 0 |
| `gateway: core` | `gateway:core` | 134 | 0 |
| `health: core` | `health:core` | 4 | 0 |
| `heartbeat: engine` | `heartbeat:engine` | 22 | 0 |
| `integration: core` | `integration:core` | 80 | 0 |
| `memory: backend` | `memory:backend` | 2 | 2 |
| `memory: postgres` | `memory:postgres` | 6 | 1 |
| `memory: lucid` | `memory:lucid` | 10 | 0 |
| `memory: hygiene` | `memory:hygiene` | 12 | 0 |
| `observability: prometheus` | `observability:prometheus` | 7 | 1 |
| `observability: core` | `observability:core` | 8 | 0 |
| `onboard: core` | `onboard:core` | 3 | 0 |
| `onboard: wizard` | `onboard:wizard` | 303 | 0 |
| `provider: deepseek` | `provider:deepseek` | 1 | 5 |
| `provider: qwen` | `provider:qwen` | 1 | 2 |
| `provider: venice` | `provider:venice` | 2 | 1 |
| `provider: glm` | `provider:glm` | 3 | 1 |
| `provider: groq` | `provider:groq` | 4 | 0 |
| `provider: core` | `provider:core` | 6 | 1 |
| `provider: reliable` | `provider:reliable` | 7 | 1 |
| `provider: bedrock` | `provider:bedrock` | 8 | 3 |
| `provider: openai` | `provider:openai` | 8 | 8 |
| `provider: openrouter` | `provider:openrouter` | 13 | 3 |
| `provider: kimi` | `provider:kimi` | 30 | 2 |
| `runtime: core` | `runtime:core` | 2 | 0 |
| `runtime: wasm` | `runtime:wasm` | 3 | 0 |
| `runtime: native` | `runtime:native` | 16 | 0 |
| `security: traits` | `security:traits` | 1 | 0 |
| `security: core` | `security:core` | 4 | 0 |
| `security: landlock` | `security:landlock` | 5 | 0 |
| `security: audit` | `security:audit` | 6 | 0 |
| `security: policy` | `security:policy` | 35 | 1 |
| `service: core` | `service:core` | 34 | 0 |
| `skills: symlink_tests` | `skills:symlink_tests` | 9 | 0 |
| `skills: core` | `skills:core` | 24 | 0 |
| `tool: pushover` | `tool:pushover` | 1 | 0 |
| `tool: schema` | `tool:schema` | 1 | 0 |
| `tool: schedule` | `tool:schedule` | 3 | 0 |
| `tool: file_read` | `tool:file_read` | 5 | 0 |
| `tool: git_operations` | `tool:git_operations` | 6 | 0 |
| `tool: shell` | `tool:shell` | 10 | 7 |
| `tool: composio` | `tool:composio` | 16 | 1 |
| `tool: core` | `tool:core` | 27 | 0 |

## Bucket C: migrate-first active labels

These legacy labels still had open refs. Migrate each open issue/PR to the canonical label before deleting the legacy label.

| Legacy label | Canonical label | Open refs to migrate |
|---|---|---|
| `memory: core` | `memory:core` | #4760, #4827, #4880 |
| `channel: cli` | `channel:cli` | #4721, #4842 |
| `observability: otel` | `observability:otel` | #6641, #6642 |
| `provider: compatible` | `provider:compatible` | #5256, #6361 |
| `security: pairing` | `security:pairing` | #6561, #6613 |
| `agent: prompt` | `agent:prompt` | #6360 |
| `channel: discord` | `channel:discord` | PR #6829 |
| `channel: nextcloud_talk` | `channel:nextcloud-talk` | #6157 |
| `channel: qq` | `channel:qq` | #2503 |
| `gateway: webhook_ingress` | `gateway:webhook_ingress` | #2467 |
| `memory: sqlite` | `memory:sqlite` | PR #6777 |
| `provider: anthropic` | `provider:anthropic` | #6678 |
| `provider: gemini` | `provider:gemini` | #4879 |
| `provider: ollama` | `provider:ollama` | #5287 |
| `tool: browser` | `tool:browser` | #6241 |
| `tool: delegate` | `tool:delegate` | PR #5530 |
| `tool: http_request` | `tool:http_request` | #5122 |

## Bucket D: holdbacks

Do not include these in a mechanical cleanup batch. They need policy, taxonomy, labeler, or stale-automation decisions first.

| Label family or label | Reason to hold |
|---|---|
| `risk: medium`, `risk:medium` | Risk label spelling and docs should be settled by policy. Current maintainer docs use `risk: medium`. |
| `type:*`, `size:*`, `priority:*`, `status:*` | Governance labels with workflow meaning. |
| `domain:*`, `area:*` | Possible board/planning taxonomy labels. |
| `documentation`, `invalid`, `wontfix`, `Testing`, `ApprovedRequest` | Default or workflow labels with historical semantics. |
| Contributor tier labels | Defined by `.github/label-policy.json`; do not remove as part of module cleanup. |
| `agent:memory_loader` | Plausible module vocabulary; decide whether to document, labeler-own, or delete. |
| `provider:cloudflare`, `provider:cohere`, `provider:fireworks`, `provider:perplexity`, `provider:qianfan`, `provider:router`, `provider:together` | Provider vocabulary that may deserve a labeler/doc decision instead of deletion. |
| `tunnel:cloudflare`, `tunnel:custom`, `tunnel:ngrok`, `tunnel:none`, `tunnel:tailscale` | Tunnel vocabulary that may deserve a labeler/doc decision instead of deletion. |

## Mutation boundary

This page is not approval to mutate GitHub. Before making live changes, prepare the exact batch:

- Labels to delete.
- Issues/PRs to relabel.
- Labels to add.
- Labels to remove.
- Verification query to run after the batch.

Then get explicit maintainer approval for that batch.
