import {
  EVENTS,
  ProbeResultSchema,
  ProbeServerEventSchema,
  SubscriptionSchema,
  type ProbeResult,
  type ProbeServerEvent,
  type Profile,
  type ServiceProfile,
  type Settings,
  type Subscription,
} from "@mglaroute/contracts";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { create } from "zustand";
import { invoke } from "../lib/invoke";

/**
 * UI/session state only. Nothing here is persisted: subscriptions, profiles,
 * settings and test results are durable data owned by the Rust commands.
 */
export type AppStore = {
  subscriptions: Subscription[];
  serviceProfiles: ServiceProfile[];
  profiles: Profile[];
  settings: Settings | null;
  probeTested: number;
  probeTotal: number;
  probeTesting: boolean;
  probeTestingIds: string[];
  probeCancelled: boolean;
  lastProbeFinishedAt: string | null;
  notice: string | null;
  selectedProfileId: string | null;
  setSubscriptions: (subscriptions: Subscription[]) => void;
  setServiceProfiles: (serviceProfiles: ServiceProfile[]) => void;
  setProfiles: (profiles: Profile[]) => void;
  setSettings: (settings: Settings) => void;
  setProbeProgress: (tested: number, total: number) => void;
  setProbeServer: (event: ProbeServerEvent) => void;
  setProbeCancelled: () => void;
  setProbeFinished: (result: ProbeResult) => void;
  setNotice: (notice: string | null) => void;
  setSelectedProfileId: (id: string | null) => void;
};

export const useAppStore = create<AppStore>((set) => ({
  subscriptions: [],
  serviceProfiles: [],
  profiles: [],
  settings: null,
  probeTested: 0,
  probeTotal: 0,
  probeTesting: false,
  probeTestingIds: [],
  probeCancelled: false,
  lastProbeFinishedAt: null,
  notice: null,
  // Session-only: which server the next Connect targets. Never persisted.
  selectedProfileId: null,
  setSubscriptions: (subscriptions) => set({ subscriptions }),
  setServiceProfiles: (serviceProfiles) => set({ serviceProfiles }),
  setProfiles: (profiles) => set({ profiles }),
  setSettings: (settings) => set({ settings }),
  setProbeProgress: (probeTested, probeTotal) => set({ probeTested, probeTotal }),
  setProbeServer: (event) =>
    set((state) => {
      const ids = new Set(state.probeTestingIds);
      if (event.status === "testing") ids.add(event.profileId);
      else ids.delete(event.profileId);
      return { probeTestingIds: [...ids], probeTesting: ids.size > 0 };
    }),
  setProbeCancelled: () => set({ probeCancelled: true }),
  setProbeFinished: (result) =>
    set({
      probeTesting: false,
      probeTestingIds: [],
      probeCancelled: result.cancelled,
      lastProbeFinishedAt: result.finishedAt,
    }),
  setNotice: (notice) => set({ notice }),
  setSelectedProfileId: (selectedProfileId) => set({ selectedProfileId }),
}));

/** Subscribe to backend progress events. Payloads are validated before use. */
export function useBackendEvents(): void {
  const setProbeProgress = useAppStore((store) => store.setProbeProgress);
  const setProbeServer = useAppStore((store) => store.setProbeServer);
  const setProbeCancelled = useAppStore((store) => store.setProbeCancelled);
  const setProbeFinished = useAppStore((store) => store.setProbeFinished);
  const setSubscriptions = useAppStore((store) => store.setSubscriptions);
  const setNotice = useAppStore((store) => store.setNotice);

  useEffect(() => {
    let active = true;
    const cleanups: Array<() => void> = [];

    const subscribe = (event: string, handler: (payload: unknown) => void) => {
      try {
        void listen(event, (received) => {
          if (active) handler(received.payload);
        }).then(
          (stop) => {
            if (active) cleanups.push(stop);
            else stop();
          },
          () => {
            // Never log native error text: it can echo subscription URLs.
          },
        );
      } catch {
        // No Tauri host (e.g. unit test); events stay inert.
      }
    };

    subscribe(EVENTS.probeProgress, (payload) => {
      const progress = payload as { tested?: unknown; total?: unknown };
      if (typeof progress?.tested === "number" && typeof progress.total === "number") {
        setProbeProgress(progress.tested, progress.total);
      }
    });

    subscribe(EVENTS.probeServer, (payload) => {
      const parsed = ProbeServerEventSchema.safeParse(payload);
      if (parsed.success) setProbeServer(parsed.data);
    });

    subscribe(EVENTS.probeCancelled, () => {
      setProbeCancelled();
    });

    subscribe(EVENTS.probeFinished, (payload) => {
      const parsed = ProbeResultSchema.safeParse(payload);
      if (parsed.success) {
        setProbeFinished(parsed.data);
        if (parsed.data.cancelled) setNotice("Test cancelled");
      }
    });

    subscribe(EVENTS.subscriptionRefreshed, (payload) => {
      const result = payload as {
        added?: unknown;
        updated?: unknown;
        absent?: unknown;
        badLines?: unknown;
      };
      if (
        typeof result?.added === "number" &&
        typeof result.updated === "number" &&
        typeof result.absent === "number" &&
        typeof result.badLines === "number"
      ) {
        setNotice(
          `Refresh complete: ${result.added} added, ${result.updated} updated, ${result.absent} absent, ${result.badLines} bad lines`,
        );
        // Re-fetch so status badges (e.g. error) update without a page remount.
        void invoke("subscription_list", SubscriptionSchema.array())
          .then((rows) => {
            if (active) setSubscriptions(rows);
          })
          .catch(() => {
            // Never log native error text: it can echo subscription URLs.
          });
      }
    });

    return () => {
      active = false;
      cleanups.forEach((stop) => stop());
    };
  }, [
    setProbeProgress,
    setProbeServer,
    setProbeCancelled,
    setProbeFinished,
    setSubscriptions,
    setNotice,
  ]);
}
