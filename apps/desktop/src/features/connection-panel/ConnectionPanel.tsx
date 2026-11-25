import { ConnectionStateSchema } from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";
import { useConnectionEvents, useConnectionStore } from "../../stores/connection.store";

const labels = {
  disconnected: "Disconnected",
  connecting: "Connecting",
  connected: "Connected",
  faulted: "Faulted",
} as const;

export function ConnectionPanel() {
  const state = useConnectionStore((store) => store.state);
  const setState = useConnectionStore((store) => store.setState);
  const setNotice = useAppStore((store) => store.setNotice);
  const [busy, setBusy] = useState(false);

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

  const connect = async () => {
    setBusy(true);
    try {
      const next = await invoke("connection_connect", ConnectionStateSchema, {
        profileId: null,
      });
      setState(next);
    } catch {
      setState({ status: "faulted", profileId: null, proxyPort: null });
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
    } catch {
      setState({ status: "faulted", profileId: null, proxyPort: null });
      setNotice("Disconnect failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Connection</CardTitle>
        <span data-testid="connection-status" className="text-sm font-medium text-slate-600">
          {labels[state.status]}
        </span>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            onClick={() => void connect()}
            disabled={busy || state.status === "connecting" || state.status === "connected"}
          >
            {state.status === "faulted" ? "Reconnect" : "Connect"}
          </Button>
          {(state.status === "connected" || state.status === "connecting") && (
            <Button variant="secondary" onClick={() => void disconnect()} disabled={busy}>
              Disconnect
            </Button>
          )}
          {state.proxyPort !== null && (
            <span className="text-sm text-slate-600">Proxy port: {state.proxyPort}</span>
          )}
        </div>
        {state.status === "faulted" && (
          <p role="alert" className="mt-3 text-sm text-red-700">
            Connection faulted. Reconnect to continue.
          </p>
        )}
        <p className="mt-3 text-xs text-slate-500">
          System proxy only. TUN and anti-DPI are unavailable in this build.
        </p>
      </CardContent>
    </Card>
  );
}
