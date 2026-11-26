import {
  AntiDpiStatusSchema,
  ProfileSchema,
  SubscriptionSchema,
} from "@mglaroute/contracts";
import { Power } from "lucide-react";
import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { ConnectionPanel } from "../features/connection-panel/ConnectionPanel";
import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { useConnectionStore } from "../stores/connection.store";

export default function Dashboard() {
  const subscriptionCount = useAppStore((store) => store.subscriptions.length);
  const serverCount = useAppStore((store) => store.profiles.length);
  const profiles = useAppStore((store) => store.profiles);
  const selectedProfileId = useAppStore((store) => store.selectedProfileId);
  const connection = useConnectionStore((store) => store.state);
  const setSubscriptions = useAppStore((store) => store.setSubscriptions);
  const setProfiles = useAppStore((store) => store.setProfiles);
  const tested = useAppStore((store) => store.probeTested);
  const total = useAppStore((store) => store.probeTotal);
  const probeTesting = useAppStore((store) => store.probeTesting);
  const finishedAt = useAppStore((store) => store.lastProbeFinishedAt);
  const [antiDpi, setAntiDpi] = useState("unknown");

  // Hydrate metric counts from durable storage on every mount (no auto-connect).
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

  // Backend connection state wins when connected; otherwise show the active
  // target chosen by the row power action (session-only, like the selection).
  const activeId = connection.profileId ?? selectedProfileId;
  const active = profiles.find((profile) => profile.id === activeId) ?? null;
  const activeState = !active ? "none" : connection.status === "connected" ? "connected" : "active";

  return (
    <div className="grid gap-4">
      <ConnectionPanel />
      <Card>
        <CardHeader>
          <CardTitle>Current server</CardTitle>
        </CardHeader>
        <CardContent>
          <div
            data-testid="active-server"
            data-state={activeState}
            className="flex items-center gap-2"
          >
            <Power aria-hidden="true" className="h-4 w-4" />
            {active ? (
              <span
                className={connection.status === "connected" ? "text-emerald-600" : "text-slate-600"}
              >
                {active.remarks ?? active.serverId} — {connection.status === "connected" ? "Connected" : "Active"}
              </span>
            ) : (
              <span className="text-slate-500">No active server</span>
            )}
          </div>
        </CardContent>
      </Card>
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
          {probeTesting ? (
            <p>Testing...</p>
          ) : total > 0 ? (
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
