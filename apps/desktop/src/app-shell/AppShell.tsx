import { ProfileSchema, SubscriptionSchema } from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { ConnectionPanel } from "../features/connection-panel/ConnectionPanel";
import { invoke } from "../lib/invoke";
import Servers from "../pages/Servers";
import { useAppStore } from "../stores/app.store";
import { BottomNav } from "./BottomNav";
import { GearSheet } from "./GearSheet";
import { ServerRail } from "./ServerRail";

type BottomTab = "connect" | "servers" | "more";

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
  const [bottomTab, setBottomTab] = useState<BottomTab>("connect");

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

  // Portrait nav tracks the last tap, not scroll position. `scrollIntoView` is
  // optional so jsdom (and any WebView without it) never throws on a nav tap.
  const scrollToZone = (testId: string) => {
    document.querySelector(`[data-testid="${testId}"]`)?.scrollIntoView?.({
      behavior: "smooth",
      block: "start",
    });
  };
  const onConnect = () => {
    setBottomTab("connect");
    scrollToZone("right-zone");
  };
  const onServers = () => {
    setBottomTab("servers");
    scrollToZone("center-zone");
  };
  const onMore = () => {
    setBottomTab("more");
    setSheetOpen(true);
  };

  return (
    <div className="flex h-[100dvh] min-h-0 w-full flex-col overflow-hidden bg-background text-foreground md:grid md:grid-cols-[72px_minmax(0,1fr)] lg:grid-cols-[240px_minmax(0,1fr)_320px]">
      <aside
        data-testid="rail-zone"
        className="order-2 min-h-0 min-w-0 overflow-y-auto overscroll-contain border-b border-border md:col-start-1 md:row-span-2 md:row-start-1 md:w-[72px] md:border-b-0 md:border-r lg:order-1 lg:w-auto lg:row-span-1"
      >
        <ServerRail onOpenSettings={() => setSheetOpen(true)} />
      </aside>

      <main
        data-testid="center-zone"
        className="order-3 min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain p-4 pb-[calc(72px+env(safe-area-inset-bottom))] sm:pb-4 md:col-start-2 md:row-start-2 lg:order-2 lg:row-start-1 lg:p-6 lg:pb-6"
      >
        <Servers />
      </main>

      <section
        data-testid="right-zone"
        className="order-1 min-h-0 min-w-0 overflow-y-auto overscroll-contain border-t border-border p-4 md:col-start-2 md:row-start-1 md:border-l md:border-t-0 lg:order-3 lg:col-start-3"
      >
        <ConnectionPanel />
      </section>

      <GearSheet open={sheetOpen} onOpenChange={setSheetOpen} />

      <BottomNav
        active={bottomTab}
        onConnect={onConnect}
        onServers={onServers}
        onMore={onMore}
      />

      {notice && (
        <div
          role="status"
          className="fixed bottom-20 left-1/2 z-[60] flex max-w-sm -translate-x-1/2 items-center gap-3 rounded-md border border-border bg-card px-4 py-3 text-sm text-foreground shadow-lg sm:bottom-4"
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
