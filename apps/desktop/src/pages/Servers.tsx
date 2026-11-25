import { ProbeResultSchema, ProfileSchema } from "@mglaroute/contracts";
import { useEffect } from "react";
import { z } from "zod";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { ServerTable } from "../features/server-table/ServerTable";
import { commandErrorCode, invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";

export default function Servers() {
  const profiles = useAppStore((store) => store.profiles);
  const setProfiles = useAppStore((store) => store.setProfiles);
  const setNotice = useAppStore((store) => store.setNotice);
  const tested = useAppStore((store) => store.probeTested);
  const total = useAppStore((store) => store.probeTotal);
  const probeTesting = useAppStore((store) => store.probeTesting);
  const probeTestingIds = useAppStore((store) => store.probeTestingIds);
  const selectedProfileId = useAppStore((store) => store.selectedProfileId);
  const setSelectedProfileId = useAppStore((store) => store.setSelectedProfileId);
  const setProbeStarted = useAppStore((store) => store.setProbeStarted);

  useEffect(() => {
    let active = true;
    void invoke("profile_list", ProfileSchema.array())
      .then((rows) => {
        if (active) setProfiles(rows);
      })
      .catch(() => {
        if (active) setNotice("Could not load servers");
      });
    return () => {
      active = false;
    };
  }, [setProfiles, setNotice]);

  const runProbe = async () => {
    setProbeStarted();
    try {
      const result = await invoke("probe_run", ProbeResultSchema);
      const rows = await invoke("profile_list", ProfileSchema.array());
      setProfiles(rows);
      setNotice(
        result.cancelled ? "Test cancelled" : `Tested ${result.tested} servers`,
      );
    } catch (error) {
      const code = commandErrorCode(error);
      if (code === "busy" || code === "probe_busy") {
        setNotice("Another operation is in progress. Try again shortly.");
      } else if (code === "probe") {
        setNotice("Server test failed. Check xray and the network, then retry.");
      } else {
        setNotice("Could not run server test");
      }
    }
  };

  const stopProbe = async () => {
    try {
      await invoke("probe_cancel", z.boolean());
    } catch {
      setNotice("Could not stop server test");
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Servers</CardTitle>
        <div className="flex items-center gap-3">
          {(probeTesting || total > 0) && (
            <span className="text-xs text-slate-500">
              {tested} / {total}
            </span>
          )}
          <Button onClick={() => void runProbe()} disabled={probeTesting}>
            {probeTesting ? "Testing..." : "Run test"}
          </Button>
          {probeTesting && (
            <Button variant="secondary" onClick={() => void stopProbe()}>
              Stop test
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <ServerTable
          profiles={profiles}
          testingIds={probeTestingIds}
          selectedId={selectedProfileId}
          onSelect={setSelectedProfileId}
        />
      </CardContent>
    </Card>
  );
}
