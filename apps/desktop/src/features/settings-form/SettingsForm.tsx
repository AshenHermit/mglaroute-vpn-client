import { SettingsSchema, type Settings, type SettingsPatch } from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

type BooleanField =
  | "autoRefetchEnabled"
  | "autoTestEnabled"
  | "autoReconnectEnabled"
  | "systemProxyEnabled";

// `refetchIntervalMinutes` stays in the settings contract/storage but is not
// surfaced here: refetch cadence is per subscription (`autoRefetchMinutes`).
type NumberField =
  | "testIntervalMinutes"
  | "testTimeoutMs"
  | "testConcurrency"
  | "reconnectCooldownSeconds"
  | "localProxyPort";

type TextField = "pingUrl" | "proxyExclusions";

export function SettingsForm() {
  const setSettings = useAppStore((store) => store.setSettings);
  const setNotice = useAppStore((store) => store.setNotice);
  const [form, setForm] = useState<Settings | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);

  useEffect(() => {
    let active = true;
    void invoke("settings_get", SettingsSchema)
      .then((settings) => {
        if (active) {
          setForm(settings);
          setLoadFailed(false);
        }
      })
      .catch(() => {
        if (active) {
          setLoadFailed(true);
          setNotice("Could not load settings");
        }
      });
    return () => {
      active = false;
    };
  }, [setNotice]);

  const reload = () => {
    setLoadFailed(false);
    setForm(null);
    void invoke("settings_get", SettingsSchema)
      .then((settings) => setForm(settings))
      .catch(() => {
        setLoadFailed(true);
        setNotice("Could not load settings");
      });
  };

  const persist = async (patch: SettingsPatch) => {
    try {
      const saved = await invoke("settings_update", SettingsSchema, patch);
      setForm(saved);
      setSettings(saved);
    } catch {
      setNotice("Could not save settings");
    }
  };

  if (loadFailed) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Settings</CardTitle>
        </CardHeader>
        <CardContent>
          <p role="alert" className="text-sm text-red-700">
            Could not load settings. Try again once other operations finish.
          </p>
          <Button className="mt-3" onClick={reload}>
            Retry
          </Button>
        </CardContent>
      </Card>
    );
  }

  if (!form) {
    return <p className="text-sm text-slate-500">Loading settings…</p>;
  }

  const toggle = (field: BooleanField) => (checked: boolean) => {
    setForm({ ...form, [field]: checked });
    void persist({ [field]: checked });
  };

  const setNumber = (field: NumberField) => (value: string) => {
    const parsed = Number.parseInt(value, 10);
    setForm({ ...form, [field]: Number.isNaN(parsed) ? 0 : parsed });
  };

  const setText = (field: TextField) => (value: string) => {
    setForm({ ...form, [field]: value });
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Settings</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid gap-4 sm:grid-cols-2">
          <Switch
            checked={form.autoRefetchEnabled}
            label="Auto refetch enabled"
            onCheckedChange={toggle("autoRefetchEnabled")}
          />
          <Switch
            checked={form.autoTestEnabled}
            label="Auto test enabled"
            onCheckedChange={toggle("autoTestEnabled")}
          />
          <Switch
            checked={form.autoReconnectEnabled}
            label="Auto reconnect enabled"
            onCheckedChange={toggle("autoReconnectEnabled")}
          />
          <Switch
            checked={form.systemProxyEnabled}
            label="System proxy enabled"
            onCheckedChange={toggle("systemProxyEnabled")}
          />

          <NumberInput
            label="Test interval (minutes)"
            value={form.testIntervalMinutes}
            onChange={setNumber("testIntervalMinutes")}
          />
          <NumberInput
            label="Test timeout (ms)"
            value={form.testTimeoutMs}
            onChange={setNumber("testTimeoutMs")}
          />
          <NumberInput
            label="Test concurrency"
            value={form.testConcurrency}
            onChange={setNumber("testConcurrency")}
          />
          <NumberInput
            label="Reconnect cooldown (seconds)"
            value={form.reconnectCooldownSeconds}
            onChange={setNumber("reconnectCooldownSeconds")}
          />
          <NumberInput
            label="Local proxy port"
            value={form.localProxyPort}
            onChange={setNumber("localProxyPort")}
          />
          <div>
            <label htmlFor="ping-url" className="mb-1 block text-xs text-slate-600">
              Ping URL
            </label>
            <Input
              id="ping-url"
              value={form.pingUrl}
              onChange={(event) => setText("pingUrl")(event.target.value)}
            />
          </div>
          <div>
            <label htmlFor="proxy-exclusions" className="mb-1 block text-xs text-slate-600">
              Proxy exclusions
            </label>
            <Input
              id="proxy-exclusions"
              value={form.proxyExclusions}
              onChange={(event) => setText("proxyExclusions")(event.target.value)}
              placeholder="localhost;127.0.0.1"
            />
          </div>
        </div>
        <Button className="mt-4" onClick={() => void persist(form)}>
          Save settings
        </Button>
      </CardContent>
    </Card>
  );
}

function NumberInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (value: string) => void;
}) {
  const id = label.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  return (
    <div>
      <label htmlFor={id} className="mb-1 block text-xs text-slate-600">
        {label}
      </label>
      <Input
        id={id}
        type="number"
        min={0}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  );
}
