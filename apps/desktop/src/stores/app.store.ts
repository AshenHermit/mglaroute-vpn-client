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
export type NoticeVariant = "info" | "success" | "error" | "loading";

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
  noticeVariant: NoticeVariant;
  noticeSeq: number;
  selectedProfileId: string | null;
  setSubscriptions: (subscriptions: Subscription[]) => void;
  setServiceProfiles: (serviceProfiles: ServiceProfile[]) => void;
  setProfiles: (profiles: Profile[]) => void;
  setSettings: (settings: Settings) => void;
  setProbeProgress: (tested: number, total: number) => void;
  setProbeServer: (event: ProbeServerEvent) => void;
  setProbeStarted: (ids: string[]) => void;
  setProbeSettled: () => void;
  setProbeCancelled: () => void;
  setProbeFinished: (result: ProbeResult) => void;
  setNotice: (notice: string | null, variant?: NoticeVariant) => void;
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
  noticeVariant: "info",
  noticeSeq: 0,
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
      if (event.status === "testing") {
        // First `testing` row of a run starts it: clear a stale cancellation
        // left by a previous stopped cycle (also covers backend auto-test).
        const starting = ids.size === 0;
        ids.add(event.profileId);
        return {
          probeTestingIds: [...ids],
          probeTesting: true,
          ...(starting && state.probeCancelled ? { probeCancelled: false } : {}),
        };
      }
      ids.delete(event.profileId);
      // Row highlights are independent from the run itself: with concurrency 1
      // the last id can be gone while `probe_run` is still pending. Only
      // `setProbeSettled` / `probe-finished` end the run; a terminal row event
      // must not re-enable Run mid-cycle.
      const highlight = { probeTestingIds: [...ids] };
      // A stop has no sample: keep the row values from the last real result.
      if (event.status === "cancelled") return highlight;
      // Patch the row from the terminal event itself. The database is written
      // only after the whole cycle, so waiting for `profile_list` would leave
      // the table stale for the rest of the run. Empty serviceResults count as
      // passing, matching the backend's `all()` over service results.
      const latencyMs = event.status === "done" ? event.latencyMs : null;
      const reachable =
        event.status === "done" && event.serviceResults.every((result) => result.reachable);
      const profiles = state.profiles.map((profile) =>
        profile.id === event.profileId ? { ...profile, latencyMs, reachable } : profile,
      );
      return { ...highlight, profiles };
    }),
  setProbeStarted: (ids) =>
    set({
      // Optimistic start: spinner and Stop button appear on click, but row
      // highlights come only from `probe-server` `testing` events so they
      // reflect the servers actually being probed at any concurrency value.
      // `runProbe` clears this state in `finally` on every exit path.
      probeTesting: true,
      probeTestingIds: [],
      probeCancelled: false,
      probeTested: 0,
      probeTotal: ids.length,
    }),
  // The invoke promise is the authoritative end of a manual run: a fast
  // failure may emit no terminal `probe-server` event at all. Keep backend
  // `probeFinished` responsible for `probeCancelled`/`lastProbeFinishedAt`.
  setProbeSettled: () => set({ probeTesting: false, probeTestingIds: [] }),
  setProbeCancelled: () => set({ probeCancelled: true }),
  setProbeFinished: (result) =>
    set({
      probeTesting: false,
      probeTestingIds: [],
      probeCancelled: result.cancelled,
      lastProbeFinishedAt: result.finishedAt,
    }),
  setNotice: (notice, variant = "info") =>
    set((state) => ({
      notice,
      noticeVariant: variant,
      noticeSeq: state.noticeSeq + 1,
    })),
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
