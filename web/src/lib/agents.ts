import { getMapKeys, listProps, patchConfig } from './api';

export interface AgentSummary {
  alias: string;
  enabled: boolean;
  modelProvider: string;
  channels: string[];
}

function entryValue(entry: { populated?: boolean; value?: unknown }): unknown {
  if (!entry.populated) return undefined;
  return entry.value;
}

/**
 * Load summaries for every configured agent. One round-trip to fetch the
 * alias list, one per alias for its fields. Suitable for dashboards and
 * pickers; not suitable for the highest-traffic page in the app.
 */
export async function loadAgentSummaries(): Promise<AgentSummary[]> {
  const { keys } = await getMapKeys('agents');
  if (keys.length === 0) return [];
  return Promise.all(
    keys.map(async (alias): Promise<AgentSummary> => {
      const { entries } = await listProps(`agents.${alias}`);
      const lookup = (suffix: string) =>
        entries.find((e) => e.path === `agents.${alias}.${suffix}`);
      const enabledEntry = lookup('enabled');
      const modelProviderEntry = lookup('model_provider');
      const channelsEntry = lookup('channels');
      return {
        alias,
        enabled: Boolean(entryValue(enabledEntry ?? { populated: false })),
        modelProvider:
          typeof entryValue(modelProviderEntry ?? { populated: false }) === 'string'
            ? (entryValue(modelProviderEntry!) as string)
            : '',
        channels: Array.isArray(entryValue(channelsEntry ?? { populated: false }))
          ? (entryValue(channelsEntry!) as string[])
          : [],
      };
    }),
  );
}

/** Flip the `enabled` flag for one agent via a JSON-Patch replace. */
export function toggleAgentEnabled(alias: string, next: boolean): Promise<unknown> {
  return patchConfig([
    {
      op: 'replace',
      path: `/agents/${alias}/enabled`,
      value: next,
    },
  ]);
}
