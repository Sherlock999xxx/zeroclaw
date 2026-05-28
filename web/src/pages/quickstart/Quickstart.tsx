import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  type ModelsResponse,
  type QuickstartError,
  type QuickstartState,
  type QuickstartStep,
  type QuickstartTypeOption,
  getCatalogModels,
  getQuickstartState,
  quickstartApply,
  quickstartDismiss,
} from "@/lib/api";

type Risk = "locked-down" | "balanced" | "yolo";
type Runtime = "tight" | "balanced" | "unbounded";
type Memory = "sqlite" | "none";

const RISK_OPTS: Risk[] = ["locked-down", "balanced", "yolo"];
const RUNTIME_OPTS: Runtime[] = ["tight", "balanced", "unbounded"];
const MEMORY_OPTS: Memory[] = ["sqlite", "none"];

interface FormState {
  providerType: string;
  providerAlias: string;
  model: string;
  apiKey: string;
  /** Empty string = user hasn't picked yet ([ ]); a preset name = [✓]. */
  risk: Risk | "";
  runtime: Runtime | "";
  memory: Memory | "";
  agentName: string;
}

const DEFAULT_FORM: FormState = {
  providerType: "",
  providerAlias: "",
  model: "",
  apiKey: "",
  risk: "",
  runtime: "",
  memory: "",
  agentName: "",
};

export default function Quickstart() {
  const navigate = useNavigate();
  const [form, setForm] = useState<FormState>(DEFAULT_FORM);
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<QuickstartError[]>([]);
  const [success, setSuccess] = useState<string | null>(null);
  const [quickstartState, setQuickstartState] = useState<QuickstartState | null>(null);
  const [catalog, setCatalog] = useState<ModelsResponse | null>(null);
  const runIdRef = useRef<string>(
    `${Date.now().toString(16)}${Math.random().toString(16).slice(2, 10)}`,
  );
  const lastStepRef = useRef<QuickstartStep | null>(null);
  const submittedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const state = await getQuickstartState();
        if (cancelled) return;
        setQuickstartState(state);
      } catch {
        /* empty pickers + error surfaces on submit */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Fetch the model catalog when the provider type changes. `live=true`
  // means render a picker; `live=false` means the input falls back to
  // free text via the empty datalist.
  useEffect(() => {
    if (!form.providerType) {
      setCatalog(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const res = await getCatalogModels(form.providerType);
        if (!cancelled) setCatalog(res);
      } catch {
        if (!cancelled) setCatalog(null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [form.providerType]);

  // The auto-trigger (route the user here on first launch with no agents)
  // is owned by `App.tsx` — see the `getQuickstartState` call there. This
  // page intentionally has no completion guard: per the Quickstart plan,
  // returning users reach `/quickstart` via the nav to create another
  // agent, so kicking them back to `/` here would break the primary
  // returning-user case (`tmp/quickstart-plan.md` §"Auto-trigger /
  // re-entry").

  // Fire a dismiss beacon when the page unmounts without a successful
  // Create. Closing the tab triggers `beforeunload`; clicking out
  // through React-Router triggers the cleanup function.
  useEffect(() => {
    const fire = () => {
      if (submittedRef.current) return;
      quickstartDismiss({
        run_id: runIdRef.current,
        surface: "web",
        last_step: lastStepRef.current,
      });
    };
    window.addEventListener("beforeunload", fire);
    return () => {
      window.removeEventListener("beforeunload", fire);
      fire();
    };
  }, []);

  const update = <K extends keyof FormState>(k: K, v: FormState[K]) => {
    setForm((f) => ({ ...f, [k]: v }));
    lastStepRef.current = stepForKey(k);
  };

  const submit = async () => {
    setBusy(true);
    setErrors([]);
    const submission = {
      model_provider: {
        mode: "fresh",
        value: {
          provider_type: form.providerType,
          alias: form.providerAlias,
          model: form.model,
          api_key: form.apiKey || null,
          base_url: null,
        },
      },
      risk_profile: { mode: "fresh", value: form.risk },
      runtime_profile: { mode: "fresh", value: form.runtime },
      memory: { mode: "fresh", value: form.memory },
      channels: [],
      agent: {
        name: form.agentName,
        system_prompt: "",
        personality_file: null,
      },
    };
    const res = await quickstartApply(submission);
    setBusy(false);
    if (res.kind === "errors") {
      setErrors(res.errors);
      return;
    }
    setSuccess(res.agent.alias);
    submittedRef.current = true;
  };

  if (success) {
    return (
      <div className="max-w-2xl mx-auto p-8">
        <h1 className="text-2xl font-bold mb-4">Quickstart complete</h1>
        <p className="mb-4">
          Created agent <code>{success}</code>. Daemon reload signalled.
        </p>
        <button
          className="px-4 py-2 rounded bg-blue-600 text-white"
          onClick={() => navigate(`/agent/${encodeURIComponent(success)}`)}
        >
          Start chatting
        </button>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto p-8 space-y-6">
      <h1 className="text-2xl font-bold">Quickstart</h1>
      <p className="text-sm opacity-80">
        Create one working agent end-to-end. Pick a provider, accept the
        balanced defaults, and start chatting.
      </p>

      <Section title="Model provider" done={isProviderDone(form)}>
        <Field label="Provider type">
          <select
            className="input"
            value={form.providerType}
            onChange={(e) => {
              const next = e.target.value;
              update("providerType", next);
              setForm((f) => ({
                ...f,
                providerAlias:
                  f.providerAlias === "" || f.providerAlias === f.providerType
                    ? next
                    : f.providerAlias,
                model: "",
              }));
            }}
          >
            <option value="" disabled>
              — pick a provider —
            </option>
            {quickstartState?.model_provider_types.map((opt: QuickstartTypeOption) => (
              <option key={opt.kind} value={opt.kind}>
                {opt.display_name}
                {opt.local ? " (local)" : ""}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Alias">
          <input
            className="input"
            value={form.providerAlias}
            onChange={(e) => update("providerAlias", e.target.value)}
          />
        </Field>
        <Field label="Model">
          <input
            className="input"
            value={form.model}
            onChange={(e) => update("model", e.target.value)}
            list="qs-model-catalog"
            placeholder={form.providerType ? "pick or type a model id" : ""}
          />
          <datalist id="qs-model-catalog">
            {catalog?.live &&
              catalog.models.map((m) => <option key={m} value={m} />)}
          </datalist>
        </Field>
        <Field label="API key">
          <input
            className="input"
            type="password"
            value={form.apiKey}
            onChange={(e) => update("apiKey", e.target.value)}
          />
        </Field>
      </Section>

      <Section title="Risk profile" done={form.risk !== ""}>
        <Radio
          options={RISK_OPTS}
          value={form.risk}
          onChange={(v) => update("risk", v as Risk)}
        />
      </Section>

      <Section title="Runtime profile" done={form.runtime !== ""}>
        <Radio
          options={RUNTIME_OPTS}
          value={form.runtime}
          onChange={(v) => update("runtime", v as Runtime)}
        />
      </Section>

      <Section title="Memory" done={form.memory !== ""}>
        <Radio
          options={MEMORY_OPTS}
          value={form.memory}
          onChange={(v) => update("memory", v as Memory)}
        />
      </Section>

      <Section title="Agent" done={form.agentName.trim() !== ""}>
        <Field label="Agent name">
          <input
            className="input"
            value={form.agentName}
            onChange={(e) => update("agentName", e.target.value)}
            placeholder="e.g. work"
          />
        </Field>
      </Section>

      {errors.length > 0 && (
        <ul className="text-red-500 text-sm space-y-1">
          {errors.map((e, i) => (
            <li key={i}>
              <code>{e.step}{e.field ? `.${e.field}` : ""}</code>: {e.message}
            </li>
          ))}
        </ul>
      )}

      <button
        className="px-4 py-2 rounded bg-blue-600 text-white disabled:opacity-50"
        disabled={busy || !allDone(form)}
        onClick={() => void submit()}
      >
        {busy ? "Creating..." : "Create"}
      </button>
    </div>
  );
}

function isProviderDone(form: FormState): boolean {
  return (
    form.providerType !== "" &&
    form.providerAlias.trim() !== "" &&
    form.model.trim() !== ""
  );
}

function allDone(form: FormState): boolean {
  return (
    isProviderDone(form) &&
    form.risk !== "" &&
    form.runtime !== "" &&
    form.memory !== "" &&
    form.agentName.trim() !== ""
  );
}

function stepForKey(key: keyof FormState): QuickstartStep {
  switch (key) {
    case "providerType":
    case "providerAlias":
    case "model":
    case "apiKey":
      return "model_provider";
    case "risk":
      return "risk_profile";
    case "runtime":
      return "runtime_profile";
    case "memory":
      return "memory";
    case "agentName":
      return "agent";
  }
}

function Section({
  title,
  children,
  done,
}: {
  title: string;
  children: React.ReactNode;
  done: boolean;
}) {
  return (
    <section className="border rounded p-4 space-y-3" style={{ borderColor: "var(--pc-border)" }}>
      <h2 className="font-semibold">
        {done ? "[✓]" : "[ ]"} {title}
      </h2>
      {children}
    </section>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <div className="text-xs uppercase opacity-70 mb-1">{label}</div>
      {children}
    </label>
  );
}

function Radio({
  options,
  value,
  onChange,
}: {
  options: readonly string[];
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex gap-2 flex-wrap">
      {options.map((o) => (
        <button
          key={o}
          type="button"
          onClick={() => onChange(o)}
          className={`px-3 py-1 rounded border ${value === o ? "bg-blue-600 text-white" : ""}`}
          style={{ borderColor: "var(--pc-border)" }}
        >
          {o}
        </button>
      ))}
    </div>
  );
}
