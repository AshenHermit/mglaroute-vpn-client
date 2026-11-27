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
import { commandErrorCode, invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";
import { SubscriptionDialog } from "./SubscriptionDialog";

const badgeStyles: Record<Subscription["status"], string> = {
  active: "bg-primary/15 text-primary",
  inactive: "bg-muted text-muted-foreground",
  error: "bg-destructive/15 text-destructive",
};

export function SubscriptionList() {
  const subscriptions = useAppStore((store) => store.subscriptions);
  const setSubscriptions = useAppStore((store) => store.setSubscriptions);
  const setNotice = useAppStore((store) => store.setNotice);
  const [intervals, setIntervals] = useState<Record<string, string>>({});
  const [dialog, setDialog] = useState<{
    mode: "add" | "edit";
    subscription: Subscription | null;
  } | null>(null);

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

  const onSaved = (saved: Subscription) => {
    setDialog(null);
    const exists = subscriptions.some((row) => row.id === saved.id);
    showRows(
      exists
        ? subscriptions.map((row) => (row.id === saved.id ? saved : row))
        : [...subscriptions, saved],
    );
  };

  const refresh = async (id: string) => {
    try {
      await invoke("subscription_refresh", RefreshResultSchema, { id });
      const rows = await invoke("subscription_list", SubscriptionSchema.array());
      showRows(rows);
    } catch (error) {
      const code = commandErrorCode(error);
      if (code === "mutation_busy") {
        setNotice("Another operation is in progress. Try again shortly.");
      } else if (code === "fetch") {
        setNotice("Could not reach subscription URL");
      } else if (code === "not_found") {
        setNotice("Subscription not found");
      } else if (code === "storage") {
        setNotice("Local storage error");
      } else {
        setNotice("Could not refresh subscription");
      }
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
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-lg font-semibold">Subscriptions</h2>
        <Button onClick={() => setDialog({ mode: "add", subscription: null })}>
          Add subscription
        </Button>
      </div>

      {subscriptions.length === 0 ? (
        <p className="text-sm text-muted-foreground">No subscriptions yet.</p>
      ) : (
        subscriptions.map((subscription) => (
          <Card key={subscription.id}>
            <CardHeader className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <CardTitle className="min-w-0 break-words">{subscription.name}</CardTitle>
                <span
                  data-testid={`subscription-status-${subscription.id}`}
                  className={`rounded px-2 py-0.5 text-xs font-semibold ${badgeStyles[subscription.status]}`}
                >
                  {subscription.status}
                </span>
              </div>
              <Button
                variant="secondary"
                aria-label={`Edit ${subscription.name}`}
                onClick={() => setDialog({ mode: "edit", subscription })}
              >
                Edit
              </Button>
            </CardHeader>
            <CardContent className="space-y-3">
              <p
                className="min-w-0 break-all text-xs text-muted-foreground"
                title={subscription.url}
              >
                {subscription.url}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button variant="secondary" onClick={() => void refresh(subscription.id)}>
                  Refresh
                </Button>
                <Button variant="danger" onClick={() => void remove(subscription.id)}>
                  Remove
                </Button>
              </div>
              <div className="flex flex-wrap items-end gap-3">
                <Switch
                  checked={subscription.enabled}
                  label={`Enabled for ${subscription.name}`}
                  onCheckedChange={(checked) => void setEnabled(subscription, checked)}
                />
                <label
                  htmlFor={`subscription-interval-${subscription.id}`}
                  className="min-w-0 break-words text-xs text-muted-foreground"
                >
                  Refetch interval for {subscription.name} (minutes)
                </label>
                <Input
                  id={`subscription-interval-${subscription.id}`}
                  aria-label={`Refetch interval for ${subscription.name} (minutes)`}
                  type="number"
                  min={0}
                  className="w-20 shrink-0"
                  value={intervals[subscription.id] ?? String(subscription.autoRefetchMinutes)}
                  onChange={(event) =>
                    setIntervals({ ...intervals, [subscription.id]: event.target.value })
                  }
                  onBlur={(event) => void commitInterval(subscription, event.target.value)}
                />
              </div>
            </CardContent>
          </Card>
        ))
      )}

      {dialog && (
        <SubscriptionDialog
          open
          mode={dialog.mode}
          subscription={dialog.subscription}
          onClose={() => setDialog(null)}
          onSaved={onSaved}
        />
      )}
    </div>
  );
}
