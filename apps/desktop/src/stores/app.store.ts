import {
  EVENTS,
  ProbeResultSchema,
  type Profile,
  type ServiceProfile,
  type Settings,
  type Subscription,
} from "@mglaroute/contracts";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { create } from "zustand";

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
  lastProbeFinishedAt: string | null;
  notice: string | null;
  setSubscriptions: (subscriptions: Subscription[]) => void;
  setServiceProfiles: (serviceProfiles: ServiceProfile[]) => void;
  setProfiles: (profiles: Profile[]) => void;
  setSettings: (settings: Settings) => void;
  setProbeProgress: (tested: number, total: number) => void;
  setProbeFinished: (finishedAt: string) => void;
  setNotice: (notice: string | null) => void;
};

export const useAppStore = create<AppStore>((set) => ({
  subscriptions: [],
  serviceProfiles: [],
  profiles: [],
  settings: null,
  probeTested: 0,
  probeTotal: 0,
  lastProbeFinishedAt: null,
  notice: null,
  setSubscriptions: (subscriptions) => set({ subscriptions }),
  setServiceProfiles: (serviceProfiles) => set({ serviceProfiles }),
  setProfiles: (profiles) => set({ profiles }),
  setSettings: (settings) => set({ settings }),
  setProbeProgress: (probeTested, probeTotal) => set({ probeTested, probeTotal }),
  setProbeFinished: (lastProbeFinishedAt) => set({ lastProbeFinishedAt }),
  setNotice: (notice) => set({ notice }),
}));

/** Subscribe to backend progress events. Payloads are validated before use. */
export function useBackendEvents(): void {
  const setProbeProgress = useAppStore((store) => store.setProbeProgress);
  const setProbeFinished = useAppStore((store) => store.setProbeFinished);
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

    subscribe(EVENTS.probeFinished, (payload) => {
      const parsed = ProbeResultSchema.safeParse(payload);
      if (parsed.success) setProbeFinished(parsed.data.finishedAt);
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
      }
    });

    return () => {
      active = false;
      cleanups.forEach((stop) => stop());
    };
  }, [setProbeProgress, setProbeFinished, setNotice]);
}
