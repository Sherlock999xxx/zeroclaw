import { useState, useEffect, useRef, useCallback } from 'react';
import {
  Activity,
  Pause,
  Play,
  ArrowDown,
  Filter,
} from 'lucide-react';
import type { SSEEvent } from '@/types/api';
import { SSEClient } from '@/lib/sse';
import { getToken } from '@/lib/auth';
import { apiOrigin, basePath } from '@/lib/basePath';
import { t } from '@/lib/i18n';

const DEFAULT_SEVERITY_MIN = 9;
const PAGE_LIMIT = 200;
const POLL_INTERVAL_MS = 3000;
const RING_CAPACITY = 2000;

const SEVERITY_OPTIONS: { label: string; value: number | '' }[] = [
  { label: 'TRACE+', value: 1 },
  { label: 'DEBUG+', value: 5 },
  { label: 'INFO+', value: 9 },
  { label: 'WARN+', value: 13 },
  { label: 'ERROR+', value: 17 },
  { label: 'Any', value: '' },
];

const CATEGORY_OPTIONS = [
  '',
  'agent',
  'channel',
  'cron',
  'memory',
  'tool',
  'provider',
  'session',
  'system',
  'internal',
];

const OUTCOME_OPTIONS = ['', 'success', 'failure', 'unknown'];

interface FilterState {
  q: string;
  severityMin: number | '';
  category: string;
  outcome: string;
  action: string;
  hideInternal: boolean;
  sinceDaemonStart: boolean;
  fieldEq: Record<string, string>;
}

function eventTypeStyle(type: string): { color: string; bg: string; border: string } {
  switch (type.toLowerCase()) {
    case 'error':
      return { color: 'var(--color-status-error)', bg: 'var(--color-status-error-alpha-08)', border: 'var(--color-status-error-alpha-20)' };
    case 'warn':
    case 'warning':
      return { color: 'var(--color-status-warning)', bg: 'var(--color-status-warning-alpha-05)', border: 'var(--color-status-warning-alpha-20)' };
    case 'tool_call':
    case 'tool_result':
    case 'tool_call_start':
      return { color: 'var(--pc-accent)', bg: 'var(--pc-accent-glow)', border: 'var(--pc-accent-dim)' };
    case 'llm_request':
      return { color: 'var(--color-status-info)', bg: 'color-mix(in srgb, var(--color-status-info) 6%, transparent)', border: 'color-mix(in srgb, var(--color-status-info) 20%, transparent)' };
    case 'agent_start':
    case 'agent_end':
      return { color: 'var(--color-status-success)', bg: 'var(--color-status-success-alpha-08)', border: 'var(--color-status-success-alpha-20)' };
    case 'message':
    case 'chat':
      return { color: 'var(--pc-accent)', bg: 'var(--pc-accent-glow)', border: 'var(--pc-accent-dim)' };
    case 'health':
    case 'status':
      return { color: 'var(--color-status-success)', bg: 'var(--color-status-success-alpha-08)', border: 'var(--color-status-success-alpha-20)' };
    default:
      return { color: 'var(--pc-text-muted)', bg: 'var(--pc-hover)', border: 'var(--pc-border)' };
  }
  if (severityNumber >= 13) {
    return {
      fg: 'var(--color-status-warning)',
      bg: 'var(--color-status-warning-alpha-05)',
      border: 'var(--color-status-warning-alpha-20)',
    };
  }
  if (severityNumber >= 9) {
    return {
      fg: 'var(--color-status-info)',
      bg: 'color-mix(in srgb, var(--color-status-info) 6%, transparent)',
      border: 'color-mix(in srgb, var(--color-status-info) 20%, transparent)',
    };
  }
  return {
    fg: 'var(--pc-text-muted)',
    bg: 'var(--pc-hover)',
    border: 'var(--pc-border)',
  };
}

interface LogEntry { id: string; event: SSEEvent; }

const LOG_STORAGE_KEY = 'zeroclaw_live_logs';
const MAX_PERSISTED_LOGS = 200;

function loadPersistedLogs(): LogEntry[] {
  try {
    return new Date(raw).toLocaleTimeString(undefined, { hour12: false });
  } catch {
    return raw;
  }
}

function persistLogs(entries: LogEntry[]): void {
  try {
    sessionStorage.setItem(LOG_STORAGE_KEY, JSON.stringify(entries.slice(-MAX_PERSISTED_LOGS)));
  } catch { /* QuotaExceeded */ }
}

export default function Logs() {
  const [filter, setFilter] = useState<FilterState>(DEFAULT_FILTER);
  const [events, setEvents] = useState<LogEvent[]>([]);
  const [daemonStartedAt, setDaemonStartedAt] = useState('');
  const [attributionKeys, setAttributionKeys] = useState<string[]>([]);
  const [cursorOlder, setCursorOlder] = useState<[string, string] | null>(null);
  const [atEnd, setAtEnd] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [addingField, setAddingField] = useState(false);

  // Persist logs to sessionStorage so they survive tab switches
  useEffect(() => { persistLogs(entries); }, [entries]);

  // Keep pausedRef in sync
  useEffect(() => { pausedRef.current = paused; }, [paused]);

  useEffect(() => {
    // Fetch recent event history so logs are visible even if the tab was closed
    const token = getToken();
    const headers: Record<string, string> = {};
    if (token) headers['Authorization'] = `Bearer ${token}`;

    fetch(`${apiOrigin}${basePath}/api/events/history`, { headers })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then(({ events }: { events: SSEEvent[] }) => {
        if (!Array.isArray(events) || events.length === 0) return;
        const historical: LogEntry[] = events.map((evt, i) => ({
          id: `hist-${i}`,
          event: { ...evt, type: evt.type ?? 'unknown' },
        }));
        setEntries((prev) => {
          // Deduplicate: keep history entries older than the earliest existing entry
          const earliest = prev[0]?.event.timestamp;
          const fresh = earliest
            ? historical.filter((e) => !e.event.timestamp || e.event.timestamp < earliest)
            : historical;
          const merged = [...fresh, ...prev];
          return merged.length > 500 ? merged.slice(-500) : merged;
        });
        entryIdRef.current += events.length;
      })
      .catch(() => {}); // History is best-effort

    const client = new SSEClient();

    client.onConnect = () => {
      setConnected(true);
    };

    client.onError = () => {
      setConnected(false);
    };

    client.onEvent = (event: SSEEvent) => {
      if (pausedRef.current) return;
      entryIdRef.current += 1;
      const entry: LogEntry = {
        id: `log-${entryIdRef.current}`,
        event,
      };
      setEntries((prev) => {
        const next = [...prev, entry];
        return next.length > 500 ? next.slice(-500) : next;
      });
    };
    client.connect();
    sseRef.current = client;
    return () => {
      client.disconnect();
    };
  }, []);

  const initialLoad = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const sinceTs = filterRef.current.sinceDaemonStart
        ? daemonStartedAtRef.current || undefined
        : undefined;
      const response = await fetchLogs(buildQueryParams(filterRef.current, { sinceTs }));
      setEvents(response.events);
      setCursorOlder(response.next_cursor);
      setAtEnd(response.at_end);
      setAttributionKeys(response.attribution_keys ?? []);
      setDaemonStartedAt(response.daemon_started_at);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void initialLoad();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // One incremental fetch — fetch newer-than-newest, append. Exposed via
  // a ref so the Pause/Resume button can fire it inline on Resume to
  // close the gap immediately instead of waiting up to POLL_INTERVAL_MS
  // for the next scheduled tick.
  const tickRef = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      if (cancelled || pausedRef.current) return;
      const newest = eventsRef.current[0];
      const sinceTs = newest
        ? newest['@timestamp']
        : daemonStartedAtRef.current || undefined;
      try {
        const response = await fetchLogs(
          buildQueryParams(filterRef.current, { sinceTs }),
        );
        if (cancelled) return;
        if (response.events.length > 0) mergeNewer(response.events);
        if (response.daemon_started_at) setDaemonStartedAt(response.daemon_started_at);
        if (response.attribution_keys?.length) setAttributionKeys(response.attribution_keys);
      } catch {
        // Polling errors are silent — they'd cascade otherwise. Manual
        // Refresh surfaces errors prominently.
      }
    };
    tickRef.current = tick;
    const handle = window.setInterval(() => void tick(), POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(handle);
    };
  }, [mergeNewer]);

  const loadOlder = useCallback(async () => {
    if (!cursorOlder || atEnd || loadingOlder) return;
    setLoadingOlder(true);
    setError(null);
    try {
      const response = await fetchLogs(
        buildQueryParams(filterRef.current, {
          untilTs: cursorOlder[0],
          untilId: cursorOlder[1],
        }),
      );
      setEvents((prev) => {
        const byId = new Map<string, LogEvent>();
        for (const event of prev) byId.set(event.id, event);
        for (const event of response.events) if (!byId.has(event.id)) byId.set(event.id, event);
        const merged = Array.from(byId.values());
        merged.sort((left, right) =>
          right['@timestamp'].localeCompare(left['@timestamp']),
        );
        return merged.slice(0, RING_CAPACITY);
      });
      setCursorOlder(response.next_cursor);
      setAtEnd(response.at_end);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingOlder(false);
    }
  }, [atEnd, cursorOlder, loadingOlder]);

  // Filter changes invalidate the ring — re-base from the new constraints.
  const filterKey = useMemo(() => JSON.stringify(filter), [filter]);
  const skipFirstFilterRefetch = useRef(true);
  useEffect(() => {
    if (skipFirstFilterRefetch.current) {
      skipFirstFilterRefetch.current = false;
      return;
    }
    const timer = window.setTimeout(() => void initialLoad(), 200);
    return () => window.clearTimeout(timer);
  }, [filterKey, initialLoad]);

  const setFieldEq = (key: string, value: string) => {
    setFilter((prev) => {
      const next = { ...prev.fieldEq };
      if (value) next[key] = value;
      else delete next[key];
      return { ...prev, fieldEq: next };
    });
  };

  const filteredEntries = typeFilters.size === 0 ? entries : entries.filter((e) => typeFilters.has(e.event.type));

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-6 py-3 border-b animate-fade-in" style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-surface)' }}>
        <div className="flex items-center gap-3">
          <Activity className="h-5 w-5" style={{ color: 'var(--pc-accent)' }} />
          <h2 className="text-sm font-semibold uppercase tracking-wider" style={{ color: 'var(--pc-text-primary)' }}>{t('logs.live_logs')}</h2>
          <span className="text-[10px] font-mono ml-2" style={{ color: 'var(--pc-text-faint)' }}>
            {filteredEntries.length} {t('logs.events')}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              setPaused((value) => {
                const next = !value;
                // On resume, fire one immediate fetch with `since_ts =
                // newest known` so the gap between pause and resume
                // closes right away instead of waiting up to
                // POLL_INTERVAL_MS for the next scheduled tick. The
                // tick reads `pausedRef`, which is updated by React
                // after this setState commits — so defer the call to
                // the next microtask.
                if (!next) {
                  pausedRef.current = false;
                  void Promise.resolve().then(() => tickRef.current());
                }
                return next;
              });
            }}
            className="btn-electric flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold"
            style={{ background: paused ? 'var(--color-status-success)' : 'var(--color-status-warning)', color: 'white' }}
          >
            {paused ? (
              <>
                <Play className="h-3.5 w-3.5" /> {t('logs.resume')}
              </>
            ) : (
              <>
                <Pause className="h-3.5 w-3.5" /> {t('logs.pause')}
              </>
            )}
          </button>

          {/* Jump to Bottom */}
          {!autoScroll && (
            <button onClick={jumpToBottom} className="btn-electric flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold">
              <ArrowDown className="h-3.5 w-3.5" />{t('logs.jump_to_bottom')}
            </button>
          )}
        </div>
      </div>

      {/* Event type filters */}
      {allTypes.length > 0 && (
        <div className="flex items-center gap-2 px-6 py-2 border-b overflow-x-auto" style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-base)' }}>
          <Filter className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--pc-text-faint)' }} />
          <span className="text-[10px] uppercase tracking-wider flex-shrink-0" style={{ color: 'var(--pc-text-faint)' }}>{t('logs.filter_label')}:</span>
          {allTypes.map((type) => (
            <label key={type} className="flex items-center gap-1.5 cursor-pointer flex-shrink-0">
              <input
                type="checkbox"
                checked={typeFilters.has(type)}
                onChange={() => toggleTypeFilter(type)}
                className="rounded"
                style={{ accentColor: 'var(--pc-accent)' }}
              />
              <span className="text-[10px] capitalize" style={{ color: 'var(--pc-text-muted)' }}>{type}</span>
            </label>
          ))}
          {typeFilters.size > 0 && (
            <button
              onClick={() => setTypeFilters(new Set())}
              className="text-[10px] flex-shrink-0 ml-1 transition-colors"
              style={{ color: 'var(--pc-accent)' }}>
              {t('logs.clear')}
            </button>
          )}
        </div>
      )}

      {/* Informational banner — what appears here and what does not */}
      {!infoDismissed && (
        <div className="flex items-start gap-3 px-6 py-3 border-b flex-shrink-0" style={{ borderColor: 'rgba(56, 189, 248, 0.2)', background: 'rgba(56, 189, 248, 0.05)' }}>
          <div className="flex-1 text-xs" style={{ color: 'var(--pc-text-secondary)' }}>
            <span className="font-semibold" style={{ color: '#38bdf8' }}>What appears here: </span>
            agent activity over SSE — LLM requests, tool calls, agent start/end, and errors.
            {' '}<span className="font-semibold" style={{ color: 'var(--pc-text-muted)' }}>What does not: </span>
            daemon stdout and <code>RUST_LOG</code> tracing output go to the terminal or log file, not this stream.
            {' '}To see tracing logs, run the daemon with <code>RUST_LOG=info zeroclaw</code> and check your terminal.
          </div>
          <button
            onClick={() => setInfoDismissed(true)}
            className="flex-shrink-0 text-[10px] btn-icon"
            aria-label="Dismiss"
            style={{ color: 'var(--pc-text-faint)' }}
          >
            ✕
          </button>
        </div>
      )}

      {/* Log entries */}
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto p-4 space-y-2 min-h-0"
      >
        {filteredEntries.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full animate-fade-in" style={{ color: 'var(--pc-text-muted)' }}>
            <Activity className="h-10 w-10 mb-3" style={{ color: 'var(--pc-text-faint)' }} />
            <p className="text-sm">
              {paused
                ? t('logs.paused_hint')
                : t('logs.waiting_hint')}
            </p>
          </div>
        ) : (
          filteredEntries.map((entry) => {
            const { event } = entry;
            const style = eventTypeStyle(event.type);
            const detail =
              event.message ??
              event.content ??
              event.data ??
              JSON.stringify(
                Object.fromEntries(
                  Object.entries(event).filter(
                    ([k]) => k !== 'type' && k !== 'timestamp',
                  ),
                ),
              );
            return (
              <div
                key={entry.id}
                className="card rounded-xl p-3"
              >
                <div className="flex items-start gap-3">
                  <span className="text-[10px] font-mono whitespace-nowrap mt-0.5" style={{ color: 'var(--pc-text-faint)' }}>
                    {formatTimestamp(event.timestamp)}
                  </span>
                  <span
                    className="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-semibold border capitalize flex-shrink-0"
                    style={style}
                  >
                    {event.type}
                  </span>
                  <p className="text-sm break-all min-w-0" style={{ color: 'var(--pc-text-secondary)' }}>
                    {typeof detail === 'string' ? detail : JSON.stringify(detail)}
                  </p>
                </div>
              </div>
            );
          })
          )}
      </div>
      {/* Footer: connection status */}
      <div className="flex items-center justify-center gap-2 px-6 py-2 border-t flex-shrink-0" style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-surface)' }}>
        <span className="status-dot" style={
          connected ? { background: 'var(--color-status-success)', boxShadow: '0 0 6px var(--color-status-success)' } : { background: 'var(--color-status-error)', boxShadow: '0 0 6px var(--color-status-error)' }
        } />
        <span className="text-[10px]" style={{ color: 'var(--pc-text-faint)' }}>
          {connected ? t('logs.connected') : t('logs.disconnected')}
        </span>
        <span
          className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-semibold border flex-shrink-0"
          style={{ color: style.fg, borderColor: style.border, background: 'transparent' }}
        >
          {event.severity_text}
        </span>
        <span
          className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono border flex-shrink-0"
          style={{
            color: 'var(--pc-text-muted)',
            borderColor: 'var(--pc-border)',
            background: 'var(--pc-bg-base)',
          }}
        >
          {event.event.category}.{event.event.action}
        </span>
        <div className="flex-1 min-w-0">
          {hasMessage && (
            <p
              className="text-sm break-words"
              style={{ color: 'var(--pc-text-primary)' }}
            >
              {event.message}
            </p>
          )}
          {attributionEntries.length > 0 && (
            <div
              className={`${hasMessage ? 'mt-1' : ''} flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] font-mono`}
              style={{ color: 'var(--pc-text-muted)' }}
            >
              {attributionEntries.map(([key, value]) => (
                <span key={key}>
                  <span style={{ color: 'var(--pc-text-faint)' }}>{key}=</span>
                  {String(value)}
                </span>
              ))}
              {typeof attribution.duration_ms === 'number' && (
                <span>
                  <span style={{ color: 'var(--pc-text-faint)' }}>duration_ms=</span>
                  {attribution.duration_ms}
                </span>
              )}
            </div>
          )}
          {event.attributes && Object.keys(event.attributes).length > 0 && (
            <details className="mt-1">
              <summary
                className="cursor-pointer text-[10px]"
                style={{ color: 'var(--pc-text-faint)' }}
              >
                attributes ({Object.keys(event.attributes).length})
              </summary>
              <pre
                className="mt-1 p-2 rounded text-[10px] overflow-x-auto"
                style={{
                  background: 'var(--pc-bg-base)',
                  color: 'var(--pc-text-muted)',
                  borderColor: 'var(--pc-border)',
                }}
              >
                {JSON.stringify(event.attributes, null, 2)}
              </pre>
            </details>
          )}
        </div>
      </div>
    </div>
  );
}
