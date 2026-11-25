import { ProbeResultSchema, ProfileSchema } from "@mglaroute/contracts";
import { useEffect } from "react";
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
  const selectedProfileId = useAppStore((store) => store.selectedProfileId);
  const setSelectedProfileId = useAppStore((store) => store.setSelectedProfileId);

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
    try {
      const result = await invoke("probe_run", ProbeResultSchema);
      const rows = await invoke("profile_list", ProfileSchema.array());
      setProfiles(rows);
      setNotice(`Tested ${result.tested} servers`);
    } catch (error) {
      const code = commandErrorCode(error);
      if (code === "busy") {
        setNotice("Another operation is in progress. Try again shortly.");
      } else if (code === "probe") {
        setNotice("Server test failed. Check xray and the network, then retry.");
      } else {
        setNotice("Could not run server test");
      }
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Servers</CardTitle>
        <div className="flex items-center gap-3">
          {total > 0 && (
            <span className="text-xs text-slate-500">
              {tested} / {total}
            </span>
          )}
          <Button onClick={() => void runProbe()}>Run test</Button>
        </div>
      </CardHeader>
      <CardContent>
        <ServerTable
          profiles={profiles}
          selectedId={selectedProfileId}
          onSelect={setSelectedProfileId}
        />
      </CardContent>
    </Card>
  );
}
