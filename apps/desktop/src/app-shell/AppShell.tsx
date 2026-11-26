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
  const onSheetOpenChange = (open: boolean) => {
    setSheetOpen(open);
    if (!open) setBottomTab("connect");
  };

  return (
    <div className="flex h-[100dvh] min-h-0 w-full flex-col overflow-y-auto bg-background text-foreground sm:grid sm:grid-cols-[72px_minmax(0,1fr)] sm:grid-rows-[auto_minmax(0,1fr)] sm:overflow-hidden lg:grid-cols-[240px_minmax(0,1fr)_320px] lg:grid-rows-1">
      <aside
        data-testid="rail-zone"
        className="order-2 min-w-0 border-b border-border sm:col-start-1 sm:row-span-2 sm:row-start-1 sm:min-h-0 sm:w-[72px] sm:overflow-y-auto sm:overscroll-contain sm:border-b-0 sm:border-r lg:order-1 lg:w-auto lg:row-span-1"
      >
        <ServerRail onOpenSettings={onMore} />
      </aside>

      <main
        data-testid="center-zone"
        className="order-3 min-w-0 p-4 pb-[calc(72px+env(safe-area-inset-bottom))] sm:col-start-2 sm:row-start-2 sm:min-h-0 sm:overflow-y-auto sm:overscroll-contain sm:pb-4 lg:order-2 lg:row-start-1 lg:p-6 lg:pb-6"
      >
        <Servers />
      </main>

      <section
        data-testid="right-zone"
        className="order-1 min-w-0 border-t border-border p-4 pb-[calc(72px+env(safe-area-inset-bottom))] sm:col-start-2 sm:row-start-1 sm:min-h-0 sm:overflow-y-auto sm:overscroll-contain sm:border-l sm:border-t-0 sm:pb-4 lg:order-3 lg:col-start-3"
      >
        <ConnectionPanel />
      </section>

      <GearSheet open={sheetOpen} onOpenChange={onSheetOpenChange} />

      <BottomNav
        active={bottomTab}
        onConnect={onConnect}
        onServers={onServers}
        onMore={onMore}
      />
    </div>
  );
}

export default AppShell;
