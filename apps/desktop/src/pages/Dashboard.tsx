import { AntiDpiStatusSchema } from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { ConnectionPanel } from "../features/connection-panel/ConnectionPanel";
import { invoke } from "../lib/invoke";
import { useAppStore, useBackendEvents } from "../stores/app.store";

export default function Dashboard() {
  useBackendEvents();
  const subscriptionCount = useAppStore((store) => store.subscriptions.length);
  const serverCount = useAppStore((store) => store.profiles.length);
  const tested = useAppStore((store) => store.probeTested);
  const total = useAppStore((store) => store.probeTotal);
  const finishedAt = useAppStore((store) => store.lastProbeFinishedAt);
  const [antiDpi, setAntiDpi] = useState("unknown");

  useEffect(() => {
    let active = true;
    void invoke("anti_dpi_status", AntiDpiStatusSchema)
      .then((status) => {
        if (active) setAntiDpi(status.status);
      })
      .catch(() => {
        if (active) setAntiDpi("unsupported");
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <div className="grid gap-4">
      <ConnectionPanel />
      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle>Subscriptions</CardTitle>
          </CardHeader>
          <CardContent>{subscriptionCount}</CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Servers</CardTitle>
          </CardHeader>
          <CardContent>{serverCount}</CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Anti-DPI</CardTitle>
          </CardHeader>
          <CardContent>
            {antiDpi === "unsupported"
              ? "Unsupported in this build"
              : antiDpi}
          </CardContent>
        </Card>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>Last test cycle</CardTitle>
        </CardHeader>
        <CardContent>
          {total > 0 ? (
            <p>
              Tested {tested} / {total}
            </p>
          ) : (
            <p className="text-slate-500">No test cycle yet.</p>
          )}
          {finishedAt && <p className="text-xs text-slate-500">Finished: {finishedAt}</p>}
        </CardContent>
      </Card>
    </div>
  );
}
