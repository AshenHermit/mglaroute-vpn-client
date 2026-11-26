import {
  ConnectionStateSchema,
  EVENTS,
  type ConnectionState,
} from "@mglaroute/contracts";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { create } from "zustand";

export const disconnected: ConnectionState = {
  status: "disconnected",
  profileId: null,
  proxyPort: null,
};

type ConnectionStore = {
  state: ConnectionState;
  setState: (state: ConnectionState) => void;
};

/** Connection state is live runtime state; it is never persisted. */
export const useConnectionStore = create<ConnectionStore>((set) => ({
  state: disconnected,
  setState: (state) => set({ state }),
}));

/** Subscribe to Rust's authoritative connection-state events. */
export function useConnectionEvents(): void {
  const setState = useConnectionStore((store) => store.setState);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen(EVENTS.connectionState, (event) => {
      if (!active) return;
      const parsed = ConnectionStateSchema.safeParse(event.payload);
      if (parsed.success) setState(parsed.data);
    }).then(
      (stop) => {
        if (active) unlisten = stop;
        else stop();
      },
      () => {
        // No payload in the error: Tauri errors may carry URL data.
      },
    );
    return () => {
      active = false;
      unlisten?.();
    };
  }, [setState]);
}
