import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  type QuickstartError,
  type QuickstartStep,
  getQuickstartState,
  quickstartApply,
  quickstartDismiss,
} from "@/lib/api";

type Risk = "locked-down" | "balanced" | "yolo";
type Runtime = "tight" | "balanced" | "unbounded";
type Memory = "sqlite" | "none";

const RISK_OPTS: Risk[] = ["locked-down", "balanced", "yolo"];
const RUNTIME_OPTS: Runtime[] = ["tight", "balanced", "unbounded"];

interface FormState {
  providerType: string;
  providerAlias: string;
  defaultModel: string;
  apiKey: string;
  risk: Risk;
  runtime: Runtime;
  memory: Memory;
  agentName: string;
}

const DEFAULT_FORM: FormState = {
  providerType: "anthropic",
  providerAlias: "anthropic",
  defaultModel: "claude-sonnet-4-5",
  apiKey: "",
  risk: "balanced",
  runtime: "balanced",
  memory: "sqlite",
  agentName: "",
};

export default function Quickstart() {
  const navigate = useNavigate();
  const [form, setForm] = useState<FormState>(DEFAULT_FORM);
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<QuickstartError[]>([]);
  const [success, setSuccess] = useState<string | null>(null);
  const runIdRef = useRef<string>(
    `${Date.now().toString(16)}${Math.random().toString(16).slice(2, 10)}`,
  );
  const lastStepRef = useRef<QuickstartStep | null>(null);
  const submittedRef = useRef(false);

  useEffect(() => {
    void getQuickstartState().then((s) => {
      if (s.quickstart_completed || s.agents.length > 0) {
        navigate("/", { replace: true });
      }
    });
  }, [navigate]);

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
        kind: "fresh",
        provider_type: form.providerType,
        alias: form.providerAlias,
        default_model: form.defaultModel,
        api_key: form.apiKey || null,
        base_url: null,
      },
      risk_profile: { kind: "fresh", value: form.risk },
      runtime_profile: { kind: "fresh", value: form.runtime },
      memory: { kind: "fresh", value: { kind: form.memory } },
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

      <Section title="Model provider">
        <Field label="Provider type">
          <input
            className="input"
            value={form.providerType}
            onChange={(e) => update("providerType", e.target.value)}
          />
        </Field>
        <Field label="Alias">
          <input
            className="input"
            value={form.providerAlias}
            onChange={(e) => update("providerAlias", e.target.value)}
          />
        </Field>
        <Field label="Default model">
          <input
            className="input"
            value={form.defaultModel}
            onChange={(e) => update("defaultModel", e.target.value)}
          />
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

      <Section title="Risk profile">
        <Radio
          options={RISK_OPTS}
          value={form.risk}
          onChange={(v) => update("risk", v as Risk)}
        />
      </Section>

      <Section title="Runtime profile">
        <Radio
          options={RUNTIME_OPTS}
          value={form.runtime}
          onChange={(v) => update("runtime", v as Runtime)}
        />
      </Section>

      <Section title="Memory">
        <Radio
          options={["sqlite", "none"]}
          value={form.memory}
          onChange={(v) => update("memory", v as Memory)}
        />
      </Section>

      <Section title="Agent">
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
        disabled={busy || !form.agentName.trim()}
        onClick={() => void submit()}
      >
        {busy ? "Creating..." : "Create"}
      </button>
    </div>
  );
}

function stepForKey(key: keyof FormState): QuickstartStep {
  switch (key) {
    case "providerType":
    case "providerAlias":
    case "defaultModel":
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

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border rounded p-4 space-y-3" style={{ borderColor: "var(--pc-border)" }}>
      <h2 className="font-semibold">[✓] {title}</h2>
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
