import { AntiDpiStatusSchema } from "@mglaroute/contracts";
import { Settings } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "../lib/invoke";
import { cn } from "../lib/utils";
import { useAppStore } from "../stores/app.store";
import { useUiStore } from "../stores/ui.store";

function RailButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "flex min-h-11 items-center rounded-md px-3 text-left text-sm text-muted-foreground",
        "transition-colors hover:bg-muted hover:text-foreground",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        active && "bg-muted text-primary",
      )}
    >
      {children}
    </button>
  );
}

/** Left navigation rail: real filters only, real durable-data counters. */
export function ServerRail({ onOpenSettings }: { onOpenSettings: () => void }) {
  const subscriptions = useAppStore((store) => store.subscriptions);
  const serverCount = useAppStore((store) => store.profiles.length);
  const filter = useUiStore((state) => state.filter);
  const setFilter = useUiStore((state) => state.setFilter);
  const [antiDpi, setAntiDpi] = useState("unknown");

  useEffect(() => {
    let active = true;
    void invoke("anti_dpi_status", AntiDpiStatusSchema)
      .then((status) => {
        if (active) setAntiDpi(status.status);
      })
      .catch(() => {
        // Only the whitelisted `status` is rendered; never native error text.
        if (active) setAntiDpi("unsupported");
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 bg-card p-4">
      <div className="flex items-center justify-between gap-2">
        <h1 className="text-base font-semibold tracking-wide text-foreground">mglaroute</h1>
        <button
          type="button"
          aria-label="Open settings"
          onClick={onOpenSettings}
          className={cn(
            "flex min-h-11 min-w-11 items-center justify-center rounded-md border border-border",
            "text-muted-foreground hover:bg-muted hover:text-foreground",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          )}
        >
          <Settings aria-hidden="true" className="h-5 w-5" />
        </button>
      </div>

      <nav aria-label="Server filters" className="flex flex-col gap-1">
        <RailButton active={filter.kind === "all"} onClick={() => setFilter({ kind: "all" })}>
          All servers
        </RailButton>
        <RailButton
          active={filter.kind === "favorites"}
          onClick={() => setFilter({ kind: "favorites" })}
        >
          Favorites
        </RailButton>
        <RailButton
          active={filter.kind === "reachable"}
          onClick={() => setFilter({ kind: "reachable" })}
        >
          Reachable only
        </RailButton>

        <h2 className="mt-4 px-3 text-xs font-medium uppercase tracking-wide text-muted-foreground">
          By subscription
        </h2>
        {subscriptions.map((subscription) => (
          <RailButton
            key={subscription.id}
            active={
              filter.kind === "subscription" && filter.subscriptionId === subscription.id
            }
            onClick={() =>
              setFilter({ kind: "subscription", subscriptionId: subscription.id })
            }
          >
            {subscription.name}
          </RailButton>
        ))}
      </nav>

      <div className="mt-auto flex flex-col gap-2 border-t border-border pt-3 text-xs text-muted-foreground">
        <div className="flex items-center justify-between gap-2">
          <span>Subscriptions</span>
          <span data-testid="subscription-count">{subscriptions.length}</span>
        </div>
        <div className="flex items-center justify-between gap-2">
          <span>Servers</span>
          <span data-testid="server-count">{serverCount}</span>
        </div>
        <div className="flex items-center justify-between gap-2">
          <span>Anti-DPI</span>
          <span data-testid="anti-dpi-status">{antiDpi}</span>
        </div>
      </div>
    </div>
  );
}
