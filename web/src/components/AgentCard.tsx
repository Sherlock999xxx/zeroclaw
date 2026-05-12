import { Link } from 'react-router-dom';
import { Bot, MessageSquare, Pencil, Power } from 'lucide-react';
import type { AgentSummary } from '@/lib/agents';

export interface AgentCardProps {
  agent: AgentSummary;
  toggling: boolean;
  onToggle: () => void;
}

/**
 * Self-contained card for one configured agent. Renders the alias,
 * bound model_provider, channel count, an enabled toggle, and quick
 * links to open the chat or edit the agent.
 */
export default function AgentCard({ agent, toggling, onToggle }: AgentCardProps) {
  const channelCount = agent.channels.length;
  return (
    <div
      className="rounded-2xl border p-5 transition-colors"
      style={{
        background: 'var(--pc-bg-surface)',
        borderColor: 'var(--pc-border)',
      }}
    >
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-2 min-w-0">
          <div
            className="h-9 w-9 rounded-xl flex-shrink-0 flex items-center justify-center"
            style={{ background: 'var(--pc-accent-glow)' }}
          >
            <Bot className="h-4 w-4" style={{ color: 'var(--pc-accent)' }} />
          </div>
          <div className="min-w-0">
            <p
              className="text-sm font-semibold truncate"
              style={{ color: 'var(--pc-text-primary)' }}
            >
              {agent.alias}
            </p>
            <p
              className="text-xs truncate"
              style={{ color: 'var(--pc-text-muted)' }}
            >
              {agent.modelProvider || 'no model_provider set'}
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={onToggle}
          disabled={toggling}
          className="flex items-center gap-1 px-2 py-1 rounded-lg text-[10px] font-medium transition-colors disabled:opacity-50"
          style={{
            background: agent.enabled
              ? 'var(--color-status-success-alpha-08)'
              : 'var(--pc-bg-elevated)',
            color: agent.enabled
              ? 'var(--color-status-success)'
              : 'var(--pc-text-muted)',
            border: '1px solid',
            borderColor: agent.enabled
              ? 'var(--color-status-success-alpha-20)'
              : 'var(--pc-border)',
          }}
          aria-pressed={agent.enabled}
          aria-label={agent.enabled ? 'Disable agent' : 'Enable agent'}
        >
          <Power className="h-3 w-3" />
          {agent.enabled ? 'enabled' : 'disabled'}
        </button>
      </div>

      <p className="text-xs mb-4" style={{ color: 'var(--pc-text-muted)' }}>
        {channelCount === 0
          ? 'No channels bound'
          : channelCount === 1
            ? '1 channel bound'
            : `${channelCount} channels bound`}
      </p>

      <div className="flex items-center gap-2">
        <Link
          to={`/agent/${encodeURIComponent(agent.alias)}`}
          className="btn-electric flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-xl text-xs"
        >
          <MessageSquare className="h-3.5 w-3.5" />
          Open chat
        </Link>
        <Link
          to={`/config/agents/${encodeURIComponent(agent.alias)}`}
          className="btn-secondary flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-xl text-xs"
        >
          <Pencil className="h-3.5 w-3.5" />
          Edit
        </Link>
      </div>
    </div>
  );
}
