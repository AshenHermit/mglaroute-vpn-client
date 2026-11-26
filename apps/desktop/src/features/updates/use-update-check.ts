import { UpdateInfoSchema } from "@mglaroute/contracts";
import { useEffect, useRef } from "react";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

/** Six hours, in milliseconds. */
export const UPDATE_CHECK_INTERVAL_MS = 21600000;

/**
 * Session-only, quiet update check. Runs on mount and every interval tick;
 * a failed check stores nothing and shows nothing, and simply retries on the
 * next tick. A version is announced at most once per browser session.
 */
export function useUpdateCheck(): void {
  const setUpdate = useAppStore((store) => store.setUpdate);
  const setNotice = useAppStore((store) => store.setNotice);
  const announced = useRef(new Set<string>());

  useEffect(() => {
    let active = true;

    const check = () => {
      void invoke("update_check", UpdateInfoSchema.nullable())
        .then((info) => {
          if (!active) return;
          setUpdate(info);
          if (info && !announced.current.has(info.version)) {
            announced.current.add(info.version);
            setNotice(`Update available: ${info.version}`);
          }
        })
        .catch(() => {
          // Silent by design: no notice, no error state. Retry next tick.
          // Native error text never reaches the UI.
        });
    };

    check();
    const timer = setInterval(check, UPDATE_CHECK_INTERVAL_MS);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [setUpdate, setNotice]);
}
