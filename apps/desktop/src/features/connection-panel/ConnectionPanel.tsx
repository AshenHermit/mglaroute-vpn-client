import { ConnectionStateSchema, type Profile } from "@mglaroute/contracts";
import { Power } from "lucide-react";
import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { commandErrorCode, invoke } from "../../lib/invoke";
import { cn } from "../../lib/utils";
import { useAppStore } from "../../stores/app.store";
import { useConnectionEvents, useConnectionStore } from "../../stores/connection.store";

const labels = {
  disconnected: "Disconnected",
  connecting: "Connecting",
  connected: "Connected",
  faulted: "Faulted",
} as const;

/** Client-side elapsed time only; the backend never reports session duration. */
function formatElapsed(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0");
  const rest = (seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${rest}`;
}

function Fact({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId?: string;
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <dt className="text-muted-foreground">{label}</dt>
      <dd data-testid={testId} className="break-words text-right text-foreground">
        {value}
      </dd>
    </div>
  );
}

function latencyLabel(profile: Profile | null): string {
  return profile !== null && profile.latencyMs !== null
    ? `${profile.latencyMs} ms`
    : "—";
}

export function ConnectionPanel() {
  const state = useConnectionStore((store) => store.state);
  const setState = useConnectionStore((store) => store.setState);
  const setNotice = useAppStore((store) => store.setNotice);
  const profiles = useAppStore((store) => store.profiles);
  const selectedProfileId = useAppStore((store) => store.selectedProfileId);
  const [busy, setBusy] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);

  useConnectionEvents();

  useEffect(() => {
    let active = true;
    void invoke("connection_status", ConnectionStateSchema)
      .then((current) => {
        if (active) setState(current);
      })
      .catch(() => {
        // Error text from the native layer may contain URLs; show nothing.
      });
    return () => {
      active = false;
    };
  }, [setState]);

  useEffect(() => {
    setElapsedSeconds(0);
    if (state.status !== "connected") return;
    const timer = window.setInterval(
      () => setElapsedSeconds((seconds) => seconds + 1),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [state.status]);

  const connect = async () => {
    setBusy(true);
    try {
      const next = await invoke("connection_connect", ConnectionStateSchema, {
        profileId: selectedProfileId,
      });
      setState(next);
    } catch (error) {
      // The connection slot belongs to another connect/disconnect. Report
      // that distinctly and keep the real state; it is not a connection fault.
      // A running probe does not block Connect, so no probe gating here.
      const code = commandErrorCode(error);
      if (code === "connection_busy") {
        setNotice("Connection operation in progress");
        return;
      }
      if (code === "port_in_use") {
        setNotice(
          "Proxy port is in use by another program. Change it in Settings or stop that program, then reconnect.",
        );
        return;
      }
      // The native layer owns the faulted state: it emits `connection-state`
      // when a core actually dies. A rejected command is not a crash.
      setNotice("Connection failed");
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    setBusy(true);
    try {
      const next = await invoke("connection_disconnect", ConnectionStateSchema);
      setState(next);
    } catch (error) {
      if (commandErrorCode(error) === "connection_busy") {
        setNotice("Connection operation in progress");
        return;
      }
      setNotice("Disconnect failed");
    } finally {
      setBusy(false);
    }
  };

  const connected = state.status === "connected";
  const connecting = state.status === "connecting";
  const powerName = connected || connecting
    ? "Disconnect"
    : state.status === "faulted"
      ? "Reconnect"
      : "Connect";
  const activeId = state.profileId ?? selectedProfileId;
  const activeProfile = profiles.find((profile) => profile.id === activeId) ?? null;
  const connectedAbsentServer = connected && activeProfile === null && activeId !== null;
  const activeState = connected ? "connected" : activeProfile ? "active" : "none";

  return (
    <Card className="flex h-auto flex-col gap-4 lg:h-full">
      <CardHeader>
        <CardTitle>Connection</CardTitle>
        <span
          data-testid="connection-status"
          className="text-sm font-medium text-muted-foreground"
        >
          {labels[state.status]}
        </span>
      </CardHeader>
      <CardContent>
        <div className="flex flex-col gap-5">
          <div className="flex flex-col items-center gap-3">
            <button
              type="button"
              aria-label={powerName}
              onClick={() => void (connected || connecting ? disconnect() : connect())}
              disabled={busy}
              className={cn(
                "flex size-28 min-h-11 min-w-11 items-center justify-center rounded-full border border-border bg-muted text-foreground transition-transform hover:scale-[1.03] active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
                connected &&
                  "border-primary/40 bg-primary/10 text-primary ring-2 ring-primary shadow-[0_0_0_8px_oklch(0.72_0.17_150_/_0.12)]",
              )}
            >
              <Power className="size-10" aria-hidden="true" />
            </button>
            <span className="text-sm text-muted-foreground">{powerName}</span>
          </div>

          <div
            data-testid="active-server"
            data-state={activeState}
            className="rounded-lg border border-border bg-background/40 p-4"
          >
            <p className="text-xs uppercase tracking-wide text-muted-foreground">
              Active server
            </p>
            <p className="mt-1 break-words text-base font-semibold text-foreground">
              {connectedAbsentServer
                ? "Connected server (not in list)"
                : (activeProfile?.remarks ?? activeProfile?.address ?? "No active server")}
            </p>
            {connectedAbsentServer ? (
              <p className="break-all text-xs text-muted-foreground">{activeId}</p>
            ) : (
              activeProfile !== null && (
                <p className="break-all text-xs text-muted-foreground">
                  {activeProfile.address}
                </p>
              )
            )}
            <dl className="mt-4 grid gap-2 text-sm">
              <Fact label="Protocol" value={activeProfile?.protocol ?? "—"} />
              <Fact label="Latency" value={latencyLabel(activeProfile)} />
              <Fact
                label="Uptime"
                value={formatElapsed(elapsedSeconds)}
                testId="connection-timer"
              />
            </dl>
            {state.proxyPort !== null && (
              <p className="mt-3 text-sm text-muted-foreground">
                Proxy port: {state.proxyPort}
              </p>
            )}
          </div>

          {state.status === "faulted" && (
            <p role="alert" className="text-sm text-destructive">
              Connection faulted. Reconnect to continue.
            </p>
          )}
          <p className="mt-auto text-xs text-muted-foreground">
            System proxy only. TUN and anti-DPI are unavailable in this build.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}
