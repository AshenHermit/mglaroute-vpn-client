import { ProfileSchema, SubscriptionSchema } from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { ConnectionPanel } from "../features/connection-panel/ConnectionPanel";
import { invoke } from "../lib/invoke";
import Servers from "../pages/Servers";
import { useAppStore } from "../stores/app.store";
import { GearSheet } from "./GearSheet";
import { ServerRail } from "./ServerRail";

/**
 * Three-zone shell. Layout/navigation only: `useBackendEvents()` stays in the
 * composition root (`App.tsx`), while durable-count hydration lives here as it
 * did on the retired Dashboard.
 */
export function AppShell() {
  const setSubscriptions = useAppStore((store) => store.setSubscriptions);
  const setProfiles = useAppStore((store) => store.setProfiles);
  const notice = useAppStore((store) => store.notice);
  const setNotice = useAppStore((store) => store.setNotice);
  const [sheetOpen, setSheetOpen] = useState(false);

  useEffect(() => {
    let active = true;
    void invoke("subscription_list", SubscriptionSchema.array())
      .then((rows) => {
        if (active) setSubscriptions(rows);
      })
      .catch(() => {
        // Never surface native error text: it may echo subscription URLs.
      });
    void invoke("profile_list", ProfileSchema.array())
      .then((rows) => {
        if (active) setProfiles(rows);
      })
      .catch(() => {
        // Error text may carry link data; stay silent.
      });
    return () => {
      active = false;
    };
  }, [setSubscriptions, setProfiles]);

  return (
    <div className="flex h-[100dvh] min-h-0 w-full flex-col overflow-hidden bg-background text-foreground lg:grid lg:grid-cols-[240px_minmax(0,1fr)_320px]">
      <aside
        data-testid="rail-zone"
        className="min-h-0 min-w-0 overflow-y-auto overscroll-contain border-b border-border lg:border-b-0 lg:border-r"
      >
        <ServerRail onOpenSettings={() => setSheetOpen(true)} />
      </aside>

      <main
        data-testid="center-zone"
        className="min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain p-4 lg:p-6"
      >
        <Servers />
      </main>

      <section
        data-testid="right-zone"
        className="min-h-0 min-w-0 overflow-y-auto overscroll-contain border-t border-border p-4 lg:border-l lg:border-t-0"
      >
        <ConnectionPanel />
      </section>

      <GearSheet open={sheetOpen} onOpenChange={setSheetOpen} />

      {notice && (
        <div
          role="status"
          className="fixed bottom-4 left-1/2 z-[60] flex max-w-sm -translate-x-1/2 items-center gap-3 rounded-md border border-border bg-card px-4 py-3 text-sm text-foreground shadow-lg"
        >
          <span>{notice}</span>
          <button
            type="button"
            aria-label="Dismiss notice"
            onClick={() => setNotice(null)}
            className="flex min-h-11 min-w-11 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            ×
          </button>
        </div>
      )}
    </div>
  );
}

export default AppShell;
