import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Activity, Filter, RefreshCw, X } from 'lucide-react';
import { apiFetch } from '@/lib/api';
import type { LogEvent, LogsQueryParams, LogsResponse } from '@/lib/api';

const DEFAULT_SEVERITY_MIN = 9;
const PAGE_LIMIT = 200;

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

interface PageState {
  events: LogEvent[];
  nextCursor: [string, string] | null;
  atEnd: boolean;
  attributionKeys: string[];
  daemonStartedAt: string;
}

const EMPTY_PAGE: PageState = {
  events: [],
  nextCursor: null,
  atEnd: false,
  attributionKeys: [],
  daemonStartedAt: '',
};

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

const DEFAULT_FILTER: FilterState = {
  q: '',
  severityMin: DEFAULT_SEVERITY_MIN,
  category: '',
  outcome: '',
  action: '',
  hideInternal: true,
  sinceDaemonStart: true,
  fieldEq: {},
};

function severityColor(severityNumber: number): { fg: string; bg: string; border: string } {
  if (severityNumber >= 17) {
    return {
      fg: 'var(--color-status-error)',
      bg: 'var(--color-status-error-alpha-08)',
      border: 'var(--color-status-error-alpha-20)',
    };
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

function formatTimestamp(raw: string): string {
  try {
    return new Date(raw).toLocaleTimeString(undefined, { hour12: false });
  } catch {
    return raw;
  }
}

function buildQueryParams(
  filter: FilterState,
  daemonStartedAt: string,
  cursor?: [string, string],
): LogsQueryParams {
  const params: LogsQueryParams = {
    limit: PAGE_LIMIT,
    hide_internal: filter.hideInternal,
  };
  if (filter.q.trim()) params.q = filter.q.trim();
  if (filter.severityMin !== '') params.severity_min = filter.severityMin;
  if (filter.category) params.category = filter.category;
  if (filter.outcome) params.outcome = filter.outcome;
  if (filter.action.trim()) params.action = filter.action.trim();
  if (filter.sinceDaemonStart && daemonStartedAt) {
    params.since_ts = daemonStartedAt;
  }
  if (cursor) {
    params.until_ts = cursor[0];
    params.until_id = cursor[1];
  }
  const fieldEq: Record<string, string> = {};
  for (const [key, value] of Object.entries(filter.fieldEq)) {
    if (value.trim()) fieldEq[key] = value.trim();
  }
  if (Object.keys(fieldEq).length > 0) params.field_eq = fieldEq;
  return params;
}

function loadLogs(params: LogsQueryParams): Promise<LogsResponse> {
  const usp = new URLSearchParams();
  const { field_eq, ...rest } = params;
  for (const [key, value] of Object.entries(rest)) {
    if (value === undefined || value === null || value === '') continue;
    usp.set(key, String(value));
  }
  if (field_eq) {
    for (const [key, value] of Object.entries(field_eq)) {
      if (value === undefined || value === null || value === '') continue;
      usp.set(key, value);
    }
  }
  const qs = usp.toString();
  return apiFetch<LogsResponse>(`/api/logs${qs ? `?${qs}` : ''}`);
}

export default function Logs() {
  const [filter, setFilter] = useState<FilterState>(DEFAULT_FILTER);
  const [page, setPage] = useState<PageState>(EMPTY_PAGE);
  const [loading, setLoading] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const filterRef = useRef(filter);
  filterRef.current = filter;

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const current = filterRef.current;
      // First fetch (no cursor) so we don't have daemon_started_at yet.
      // Fall back to current daemon_started_at from the page state if we
      // already loaded once; otherwise omit since_ts and let the server
      // tell us what to use on the next refresh.
      const response = await loadLogs(
        buildQueryParams(current, page.daemonStartedAt),
      );
      setPage({
        events: response.events,
        nextCursor: response.next_cursor,
        atEnd: response.at_end,
        attributionKeys: response.attribution_keys ?? [],
        daemonStartedAt: response.daemon_started_at,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [page.daemonStartedAt]);

  useEffect(() => {
    void refresh();
    // refresh is intentionally invoked on mount only; subsequent refreshes
    // happen via the Refresh button or filter changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loadOlder = useCallback(async () => {
    if (!page.nextCursor || page.atEnd || loadingOlder) return;
    setLoadingOlder(true);
    setError(null);
    try {
      const response = await loadLogs(
        buildQueryParams(filterRef.current, page.daemonStartedAt, page.nextCursor),
      );
      setPage((prev) => ({
        events: [...prev.events, ...response.events],
        nextCursor: response.next_cursor,
        atEnd: response.at_end,
        attributionKeys: response.attribution_keys ?? prev.attributionKeys,
        daemonStartedAt: prev.daemonStartedAt || response.daemon_started_at,
      }));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingOlder(false);
    }
  }, [loadingOlder, page.atEnd, page.daemonStartedAt, page.nextCursor]);

  // Re-fetch when filters change (debounced).
  const filterKey = useMemo(() => JSON.stringify(filter), [filter]);
  const skipFirstRefetch = useRef(true);
  useEffect(() => {
    if (skipFirstRefetch.current) {
      skipFirstRefetch.current = false;
      return;
    }
    const timer = setTimeout(() => {
      void refresh();
    }, 200);
    return () => clearTimeout(timer);
  }, [filterKey, refresh]);

  const setFieldEq = (key: string, value: string) => {
    setFilter((prev) => {
      const next = { ...prev.fieldEq };
      if (value) next[key] = value;
      else delete next[key];
      return { ...prev, fieldEq: next };
    });
  };

  const clearFieldEq = (key: string) => setFieldEq(key, '');

  const activeFieldKeys = Object.keys(filter.fieldEq).filter(
    (key) => filter.fieldEq[key],
  );

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div
        className="flex items-center justify-between px-6 py-3 border-b"
        style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-surface)' }}
      >
        <div className="flex items-center gap-3">
          <Activity className="h-5 w-5" style={{ color: 'var(--pc-accent)' }} />
          <h2
            className="text-sm font-semibold uppercase tracking-wider"
            style={{ color: 'var(--pc-text-primary)' }}
          >
            Logs
          </h2>
          <span
            className="text-[10px] font-mono ml-2"
            style={{ color: 'var(--pc-text-faint)' }}
          >
            {page.events.length} events {page.atEnd ? '(end)' : ''}
          </span>
        </div>
        <button
          onClick={() => void refresh()}
          disabled={loading}
          className="btn-electric flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold"
        >
          <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          Refresh
        </button>
      </div>

      {/* Filters */}
      <div
        className="flex flex-wrap items-center gap-3 px-6 py-3 border-b"
        style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-base)' }}
      >
        <input
          type="search"
          value={filter.q}
          onChange={(event) =>
            setFilter((prev) => ({ ...prev, q: event.target.value }))
          }
          placeholder="Search message + attributes"
          className="px-2 py-1 text-xs rounded border min-w-[220px] flex-1"
          style={{
            background: 'var(--pc-bg-surface)',
            borderColor: 'var(--pc-border)',
            color: 'var(--pc-text-primary)',
          }}
        />
        <select
          value={filter.severityMin}
          onChange={(event) =>
            setFilter((prev) => ({
              ...prev,
              severityMin:
                event.target.value === ''
                  ? ''
                  : Number.parseInt(event.target.value, 10),
            }))
          }
          className="px-2 py-1 text-xs rounded border"
          style={{
            background: 'var(--pc-bg-surface)',
            borderColor: 'var(--pc-border)',
            color: 'var(--pc-text-primary)',
          }}
        >
          {SEVERITY_OPTIONS.map((option) => (
            <option key={String(option.value)} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <select
          value={filter.category}
          onChange={(event) =>
            setFilter((prev) => ({ ...prev, category: event.target.value }))
          }
          className="px-2 py-1 text-xs rounded border"
          style={{
            background: 'var(--pc-bg-surface)',
            borderColor: 'var(--pc-border)',
            color: 'var(--pc-text-primary)',
          }}
        >
          {CATEGORY_OPTIONS.map((option) => (
            <option key={option} value={option}>
              {option || 'Any category'}
            </option>
          ))}
        </select>
        <select
          value={filter.outcome}
          onChange={(event) =>
            setFilter((prev) => ({ ...prev, outcome: event.target.value }))
          }
          className="px-2 py-1 text-xs rounded border"
          style={{
            background: 'var(--pc-bg-surface)',
            borderColor: 'var(--pc-border)',
            color: 'var(--pc-text-primary)',
          }}
        >
          {OUTCOME_OPTIONS.map((option) => (
            <option key={option} value={option}>
              {option || 'Any outcome'}
            </option>
          ))}
        </select>
        <input
          type="text"
          value={filter.action}
          onChange={(event) =>
            setFilter((prev) => ({ ...prev, action: event.target.value }))
          }
          placeholder="event.action"
          className="px-2 py-1 text-xs rounded border w-[180px]"
          style={{
            background: 'var(--pc-bg-surface)',
            borderColor: 'var(--pc-border)',
            color: 'var(--pc-text-primary)',
          }}
        />
        <label
          className="flex items-center gap-1.5 text-[11px] cursor-pointer"
          style={{ color: 'var(--pc-text-muted)' }}
        >
          <input
            type="checkbox"
            checked={filter.hideInternal}
            onChange={(event) =>
              setFilter((prev) => ({ ...prev, hideInternal: event.target.checked }))
            }
            style={{ accentColor: 'var(--pc-accent)' }}
          />
          Hide internal
        </label>
        <label
          className="flex items-center gap-1.5 text-[11px] cursor-pointer"
          style={{ color: 'var(--pc-text-muted)' }}
        >
          <input
            type="checkbox"
            checked={filter.sinceDaemonStart}
            onChange={(event) =>
              setFilter((prev) => ({
                ...prev,
                sinceDaemonStart: event.target.checked,
              }))
            }
            style={{ accentColor: 'var(--pc-accent)' }}
          />
          Since daemon start
        </label>
      </div>

      {/* Per-attribution filters */}
      {page.attributionKeys.length > 0 && (
        <div
          className="flex flex-wrap items-center gap-2 px-6 py-2 border-b"
          style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-surface)' }}
        >
          <Filter
            className="h-3.5 w-3.5 flex-shrink-0"
            style={{ color: 'var(--pc-text-faint)' }}
          />
          <span
            className="text-[10px] uppercase tracking-wider flex-shrink-0"
            style={{ color: 'var(--pc-text-faint)' }}
          >
            zeroclaw.*
          </span>
          {page.attributionKeys.map((key) => {
            const value = filter.fieldEq[key] ?? '';
            return (
              <label
                key={key}
                className="flex items-center gap-1 text-[10px]"
                style={{ color: 'var(--pc-text-muted)' }}
              >
                <span className="font-mono">{key}=</span>
                <input
                  type="text"
                  value={value}
                  onChange={(event) => setFieldEq(key, event.target.value)}
                  placeholder="…"
                  className="px-1.5 py-0.5 text-[10px] rounded border w-[110px] font-mono"
                  style={{
                    background: 'var(--pc-bg-base)',
                    borderColor: 'var(--pc-border)',
                    color: 'var(--pc-text-primary)',
                  }}
                />
                {value && (
                  <button
                    type="button"
                    onClick={() => clearFieldEq(key)}
                    style={{ color: 'var(--pc-text-faint)' }}
                    className="ml-0.5"
                    aria-label={`Clear ${key} filter`}
                  >
                    <X className="h-3 w-3" />
                  </button>
                )}
              </label>
            );
          })}
          {activeFieldKeys.length > 0 && (
            <button
              type="button"
              onClick={() =>
                setFilter((prev) => ({ ...prev, fieldEq: {} }))
              }
              className="text-[10px] ml-1"
              style={{ color: 'var(--pc-accent)' }}
            >
              clear all
            </button>
          )}
        </div>
      )}

      {/* Errors */}
      {error && (
        <div
          className="px-6 py-2 text-xs border-b"
          style={{
            color: 'var(--color-status-error)',
            background: 'var(--color-status-error-alpha-08)',
            borderColor: 'var(--color-status-error-alpha-20)',
          }}
        >
          {error}
        </div>
      )}

      {/* Events */}
      <div className="flex-1 overflow-y-auto p-4 space-y-1 min-h-0">
        {page.events.length === 0 && !loading ? (
          <div
            className="flex flex-col items-center justify-center h-full"
            style={{ color: 'var(--pc-text-muted)' }}
          >
            <Activity
              className="h-10 w-10 mb-3"
              style={{ color: 'var(--pc-text-faint)' }}
            />
            <p className="text-sm">No events match the current filters.</p>
          </div>
        ) : (
          page.events.map((event) => <LogRow key={event.id} event={event} />)
        )}
        {!page.atEnd && page.events.length > 0 && (
          <div className="flex justify-center pt-3">
            <button
              type="button"
              onClick={() => void loadOlder()}
              disabled={loadingOlder || !page.nextCursor}
              className="btn-electric px-3 py-1.5 text-xs font-semibold"
            >
              {loadingOlder ? 'Loading…' : 'Load older'}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function LogRow({ event }: { event: LogEvent }) {
  const style = severityColor(event.severity_number);
  const attribution = event.zeroclaw ?? {};
  const attributionEntries = Object.entries(attribution).filter(
    ([key, value]) => key !== 'duration_ms' && value !== '' && value !== null,
  );
  return (
    <div
      className="rounded-md px-3 py-2 border text-xs"
      style={{ borderColor: style.border, background: style.bg }}
    >
      <div className="flex items-start gap-3">
        <span
          className="font-mono whitespace-nowrap mt-0.5 text-[10px]"
          style={{ color: 'var(--pc-text-faint)' }}
        >
          {formatTimestamp(event['@timestamp'])}
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
          <p
            className="text-sm break-words"
            style={{ color: 'var(--pc-text-primary)' }}
          >
            {event.message || <em style={{ opacity: 0.5 }}>(no message)</em>}
          </p>
          {attributionEntries.length > 0 && (
            <div
              className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] font-mono"
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
