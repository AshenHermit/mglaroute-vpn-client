import { AntiDpiStatusSchema } from "@mglaroute/contracts";
import { Globe, Server, Settings, Signal, Star } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "../lib/invoke";
import { cn } from "../lib/utils";
import { useAppStore } from "../stores/app.store";
import { useUiStore } from "../stores/ui.store";

function RailButton({
  active,
  label,
  icon,
  onClick,
}: {
  active: boolean;
  label: string;
  icon?: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      title={label}
      onClick={onClick}
      className={cn(
        "flex min-h-11 min-w-11 shrink-0 items-center justify-center gap-2 rounded-md px-3 text-left text-sm text-muted-foreground",
        "sm:justify-center sm:px-0 lg:justify-start lg:px-3",
        "transition-colors hover:bg-muted hover:text-foreground",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        active && "bg-muted text-primary",
      )}
    >
      {icon}
      <span className="hidden lg:inline">{label}</span>
    </button>
  );
}

/** Left navigation rail: real filters only, real durable-data counters. */
export function ServerRail({
  onOpenSettings,
  onOpenServers,
}: {
  onOpenSettings: () => void;
  onOpenServers: () => void;
}) {
  const subscriptions = useAppStore((store) => store.subscriptions);
  const serverCount = useAppStore((store) => store.profiles.length);
  const clientVersion = useAppStore((store) => store.clientVersion);
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
        <h1 className="sr-only text-base font-semibold tracking-wide text-foreground lg:not-sr-only">
          mglaroute
        </h1>
        {clientVersion !== null && (
          <span data-testid="client-version" className="text-xs text-muted-foreground">
            v{clientVersion}
          </span>
        )}
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

      <nav aria-label="Server filters" className="flex flex-row gap-1 overflow-x-auto sm:flex-col">
        <RailButton
          active={filter.kind === "all"}
          label="All servers"
          icon={<Server aria-hidden="true" className="h-5 w-5 shrink-0" />}
          onClick={() => {
            setFilter({ kind: "all" });
            onOpenServers();
          }}
        />
        <RailButton
          active={filter.kind === "favorites"}
          label="Favorites"
          icon={<Star aria-hidden="true" className="h-5 w-5 shrink-0" />}
          onClick={() => {
            setFilter({ kind: "favorites" });
            onOpenServers();
          }}
        />
        <RailButton
          active={filter.kind === "reachable"}
          label="Reachable only"
          icon={<Signal aria-hidden="true" className="h-5 w-5 shrink-0" />}
          onClick={() => {
            setFilter({ kind: "reachable" });
            onOpenServers();
          }}
        />

        <h2 className="sr-only mt-4 px-3 text-xs font-medium uppercase tracking-wide text-muted-foreground lg:not-sr-only">
          By subscription
        </h2>
        {subscriptions.map((subscription) => (
          <RailButton
            key={subscription.id}
            active={
              filter.kind === "subscription" && filter.subscriptionId === subscription.id
            }
            label={subscription.name}
            icon={<Globe aria-hidden="true" className="h-5 w-5 shrink-0" />}
            onClick={() => {
              setFilter({ kind: "subscription", subscriptionId: subscription.id });
              onOpenServers();
            }}
          />
        ))}
      </nav>

      <div className="mt-auto hidden flex-col gap-2 border-t border-border pt-3 text-xs text-muted-foreground sm:hidden lg:flex">
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
