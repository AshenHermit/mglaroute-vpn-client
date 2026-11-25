import {
  RefreshResultSchema,
  SubscriptionSchema,
  type Subscription,
} from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

const badgeStyles: Record<Subscription["status"], string> = {
  active: "bg-emerald-100 text-emerald-700",
  inactive: "bg-slate-100 text-slate-600",
  error: "bg-red-100 text-red-700",
};

export function SubscriptionList() {
  const subscriptions = useAppStore((store) => store.subscriptions);
  const setSubscriptions = useAppStore((store) => store.setSubscriptions);
  const setNotice = useAppStore((store) => store.setNotice);
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [intervals, setIntervals] = useState<Record<string, string>>({});

  const showRows = (rows: Subscription[]) => {
    setSubscriptions(rows);
    setIntervals(
      Object.fromEntries(rows.map((row) => [row.id, String(row.autoRefetchMinutes)])),
    );
  };

  useEffect(() => {
    let active = true;
    void invoke("subscription_list", SubscriptionSchema.array())
      .then((rows) => {
        if (active) showRows(rows);
      })
      .catch(() => {
        if (active) setNotice("Could not load subscriptions");
      });
    return () => {
      active = false;
    };
    // showRows only writes local state; store setters are stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setSubscriptions, setNotice]);

  const add = async () => {
    const trimmed = url.trim();
    if (!trimmed) {
      setNotice("Subscription URL is required");
      return;
    }
    try {
      const args = name.trim() ? { url: trimmed, name: name.trim() } : { url: trimmed };
      const created = await invoke("subscription_add", SubscriptionSchema, args);
      showRows([...subscriptions, created]);
      setUrl("");
      setName("");
    } catch {
      setNotice("Could not add subscription");
    }
  };

  const refresh = async (id: string) => {
    try {
      await invoke("subscription_refresh", RefreshResultSchema, { id });
      const rows = await invoke("subscription_list", SubscriptionSchema.array());
      showRows(rows);
    } catch {
      setNotice("Could not refresh subscription");
    }
  };

  const remove = async (id: string) => {
    try {
      await invoke("subscription_remove", z.null(), { id });
      showRows(subscriptions.filter((subscription) => subscription.id !== id));
    } catch {
      setNotice("Could not remove subscription");
    }
  };

  const setEnabled = async (subscription: Subscription, enabled: boolean) => {
    try {
      const updated = await invoke("subscription_update", SubscriptionSchema, {
        id: subscription.id,
        enabled,
      });
      showRows(subscriptions.map((row) => (row.id === updated.id ? updated : row)));
    } catch {
      setNotice("Could not update subscription");
    }
  };

  const commitInterval = async (subscription: Subscription, value: string) => {
    const parsed = Number.parseInt(value, 10);
    const minutes = Number.isNaN(parsed) || parsed < 0 ? 0 : parsed;
    if (minutes === subscription.autoRefetchMinutes) {
      return;
    }
    try {
      const updated = await invoke("subscription_update", SubscriptionSchema, {
        id: subscription.id,
        autoRefetchMinutes: minutes,
      });
      showRows(subscriptions.map((row) => (row.id === updated.id ? updated : row)));
    } catch {
      setNotice("Could not update subscription");
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Subscriptions</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="mb-4 grid gap-3 sm:grid-cols-[1fr_16rem_auto] sm:items-end">
          <div>
            <label htmlFor="subscription-url" className="mb-1 block text-xs text-slate-600">
              Subscription URL
            </label>
            <Input
              id="subscription-url"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              placeholder="https://example.com/subscription"
            />
          </div>
          <div>
            <label htmlFor="subscription-name" className="mb-1 block text-xs text-slate-600">
              Name (optional)
            </label>
            <Input
              id="subscription-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <Button onClick={() => void add()}>Add subscription</Button>
        </div>

        {subscriptions.length === 0 ? (
          <p className="text-sm text-slate-500">No subscriptions yet.</p>
        ) : (
          <ul className="divide-y divide-slate-200">
            {subscriptions.map((subscription) => (
              <li key={subscription.id} className="py-2">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{subscription.name}</span>
                    <span
                      data-testid={`subscription-status-${subscription.id}`}
                      className={`rounded px-2 py-0.5 text-xs font-semibold ${badgeStyles[subscription.status]}`}
                    >
                      {subscription.status}
                    </span>
                  </div>
                  <div className="flex gap-2">
                    <Button variant="secondary" onClick={() => void refresh(subscription.id)}>
                      Refresh
                    </Button>
                    <Button variant="danger" onClick={() => void remove(subscription.id)}>
                      Remove
                    </Button>
                  </div>
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-3">
                  <Switch
                    checked={subscription.enabled}
                    label={`Enabled for ${subscription.name}`}
                    onCheckedChange={(checked) => void setEnabled(subscription, checked)}
                  />
                  <span className="text-xs text-slate-600">
                    Refetch interval for {subscription.name} (minutes)
                  </span>
                  <Input
                    aria-label={`Refetch interval for ${subscription.name} (minutes)`}
                    type="number"
                    min={0}
                    className="w-20"
                    value={intervals[subscription.id] ?? String(subscription.autoRefetchMinutes)}
                    onChange={(event) =>
                      setIntervals({ ...intervals, [subscription.id]: event.target.value })
                    }
                    onBlur={(event) => void commitInterval(subscription, event.target.value)}
                  />
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
