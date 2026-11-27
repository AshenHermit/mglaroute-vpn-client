import { UpdateInfoSchema, type UpdateInfo } from "@mglaroute/contracts";
import { useEffect, useRef } from "react";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

/** Six hours, in milliseconds. */
export const UPDATE_CHECK_INTERVAL_MS = 21600000;

/** Startup delay before the first check, in milliseconds. */
export const UPDATE_CHECK_START_DELAY_MS = 1000;

/**
 * Session-only, quiet update check. Runs shortly after mount and every
 * interval tick; a failed check stores nothing and shows nothing, and simply
 * retries on the next tick. A version is announced at most once per browser
 * session.
 */
export function useUpdateCheck(): void {
  const setUpdate = useAppStore((store) => store.setUpdate);
  const setNotice = useAppStore((store) => store.setNotice);
  const announced = useRef(new Set<string>());
  // Rust collapses network/config failures to `null`, identical to "no update".
  // Once a version has been displayed, a later `null` must not erase it and
  // strand the Install button for up to six hours.
  const displayed = useRef<UpdateInfo | null>(null);

  useEffect(() => {
    let active = true;

    const check = () => {
      void invoke("update_check", UpdateInfoSchema.nullable())
        .then((info) => {
          if (!active) return;
          if (info === null && displayed.current !== null) {
            // Indistinguishable quiet failure — keep the last known update.
            return;
          }
          displayed.current = info;
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

    const initialTimer = setTimeout(check, UPDATE_CHECK_START_DELAY_MS);
    const timer = setInterval(check, UPDATE_CHECK_INTERVAL_MS);
    return () => {
      active = false;
      clearTimeout(initialTimer);
      clearInterval(timer);
    };
  }, [setUpdate, setNotice]);
}
