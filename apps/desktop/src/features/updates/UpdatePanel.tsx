import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { CommandInvocationError, invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

/**
 * Latest-release card. Checking stays in `useUpdateCheck`; this panel only
 * presents the store result and drives the two user actions. Native error
 * text never reaches the UI — invoke() normalises it to safe codes.
 */
export function UpdatePanel() {
  const update = useAppStore((store) => store.update);
  const updateInstalling = useAppStore((store) => store.updateInstalling);
  const updateProgress = useAppStore((store) => store.updateProgress);
  const setUpdateInstalling = useAppStore((store) => store.setUpdateInstalling);
  const setNotice = useAppStore((store) => store.setNotice);

  if (!update) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Updates</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">No update available</p>
        </CardContent>
      </Card>
    );
  }

  // Only the synchronous installing flag gates the button: the store keeps
  // terminal progress events (including `done`) and never clears them, so a
  // stale progress line must not strand retry after a failed install.
  const busy = updateInstalling;

  const perform = async (action: () => Promise<unknown>) => {
    // Set synchronously on click: a second click in the same tick sees busy.
    setUpdateInstalling(true);
    try {
      await action();
    } catch (error) {
      if (error instanceof CommandInvocationError) {
        setNotice("Update failed", "error");
      }
    } finally {
      setUpdateInstalling(false);
    }
  };

  const install = () => void perform(() => invoke("update_install", z.null()));
  const openReleasePage = () =>
    void perform(() => invoke("update_open_releases", z.null(), { url: update.releaseUrl }));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Updates</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-sm font-medium">Version {update.version}</p>
        {update.notes && <p className="mt-1 text-sm text-muted-foreground">{update.notes}</p>}
        {updateProgress && (
          <p className="mt-1 text-xs text-muted-foreground" aria-live="polite">
            {updateProgress.phase}
            {updateProgress.total === null
              ? ` ${updateProgress.received} bytes`
              : ` ${updateProgress.received} / ${updateProgress.total} bytes`}
          </p>
        )}
        {update.canInstallInApp ? (
          <Button className="mt-3" disabled={busy} onClick={install}>
            Install update
          </Button>
        ) : (
          <Button className="mt-3" disabled={busy} onClick={openReleasePage}>
            Open release page
          </Button>
        )}
      </CardContent>
    </Card>
  );
}
