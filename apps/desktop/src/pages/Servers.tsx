import {
  ConnectionStateSchema,
  ProbeResultSchema,
  type Profile,
  ProfileSchema,
} from "@mglaroute/contracts";
import { useMemo, useState } from "react";
import { z } from "zod";
import { Button } from "../components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "../components/ui/card";
import { Input } from "../components/ui/input";
import { filterProfiles } from "../features/server-list/server-filter";
import { ServerDetailsDialog } from "../features/server-list/ServerDetailsDialog";
import { ServerTable } from "../features/server-table/ServerTable";
import { formatTimestamp } from "../lib/datetime";
import { commandErrorCode, invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { useConnectionStore } from "../stores/connection.store";
import { useUiStore } from "../stores/ui.store";

export default function Servers() {
  const profiles = useAppStore((store) => store.profiles);
  const setProfiles = useAppStore((store) => store.setProfiles);
  const setNotice = useAppStore((store) => store.setNotice);
  const tested = useAppStore((store) => store.probeTested);
  const total = useAppStore((store) => store.probeTotal);
  const probeTesting = useAppStore((store) => store.probeTesting);
  const probeTestingIds = useAppStore((store) => store.probeTestingIds);
  const lastProbeFinishedAt = useAppStore((store) => store.lastProbeFinishedAt);
  const selectedProfileId = useAppStore((store) => store.selectedProfileId);
  const setSelectedProfileId = useAppStore(
    (store) => store.setSelectedProfileId,
  );
  const setProbeStarted = useAppStore((store) => store.setProbeStarted);
  const setProbeSettled = useAppStore((store) => store.setProbeSettled);
  const connection = useConnectionStore((store) => store.state);
  const setConnectionState = useConnectionStore((store) => store.setState);
  const filter = useUiStore((state) => state.filter);
  const favorites = useUiStore((state) => state.favorites);
  const toggleFavorite = useUiStore((state) => state.toggleFavorite);
  const [search, setSearch] = useState("");
  const [detailsProfile, setDetailsProfile] = useState<Profile | null>(null);

  const visibleProfiles = useMemo(() => {
    const filtered = filterProfiles(profiles, filter, favorites);
    const query = search.trim().toLowerCase();
    if (!query) return filtered;
    return filtered.filter((profile) =>
      [profile.remarks, profile.address, profile.protocol].some((value) =>
        value?.toLowerCase().includes(query),
      ),
    );
  }, [profiles, filter, favorites, search]);

  const setActive = async (profileId: string) => {
    if (connection.status !== "connected") {
      setSelectedProfileId(profileId);
      return;
    }
    try {
      const next = await invoke("connection_connect", ConnectionStateSchema, {
        profileId,
      });
      setConnectionState(next);
      // Keep the old selection if the backend refused the switch.
      setSelectedProfileId(profileId);
    } catch (error) {
      const code = commandErrorCode(error);
      setNotice(
        code === "connection_busy"
          ? "Connection is busy. Try again in a moment."
          : "Connection failed",
      );
    }
  };

  const runProbe = async () => {
    setProbeStarted(profiles.map((profile) => profile.id));
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
        setNotice(
          "Server test failed. Check xray and the network, then retry.",
        );
      } else {
        setNotice("Could not run server test");
      }
    } finally {
      setProbeSettled();
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
    <Card className="h-full grid grid-rows-[auto_auto]">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle>Servers</CardTitle>
          <div className="flex items-center gap-3">
            {(probeTesting || total > 0) && (
              <span className="text-xs text-muted-foreground">
                {tested} / {total}
              </span>
            )}
            <Button onClick={() => void runProbe()} disabled={probeTesting}>
              {probeTesting ? (
                <>
                  <span
                    aria-hidden="true"
                    className="mr-2 inline-block h-3 w-3 animate-spin rounded-full border-2 border-primary-foreground border-t-transparent"
                  />
                  Testing...
                </>
              ) : (
                "Run test"
              )}
            </Button>
            {probeTesting && (
              <Button variant="secondary" onClick={() => void stopProbe()}>
                Stop test
              </Button>
            )}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Input
            type="search"
            aria-label="Search servers"
            placeholder="Search servers"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            className="min-w-0 max-w-xs flex-1"
          />
          <span className="text-xs text-muted-foreground">
            {visibleProfiles.length} shown
          </span>
        </div>
        {lastProbeFinishedAt && (
          <p
            className="truncate text-xs text-muted-foreground"
            title={lastProbeFinishedAt}
          >
            Finished: {formatTimestamp(lastProbeFinishedAt)}
          </p>
        )}
      </CardHeader>
      <CardContent>
        <ServerTable
          profiles={visibleProfiles}
          testingIds={probeTestingIds}
          selectedId={selectedProfileId}
          connectedId={connection.profileId}
          favorites={favorites}
          onToggleFavorite={toggleFavorite}
          onOpenDetails={setDetailsProfile}
          onSelect={(id) => void setActive(id)}
        />
        <ServerDetailsDialog
          profile={detailsProfile}
          connected={
            connection.status === "connected" &&
            detailsProfile?.id === connection.profileId
          }
          onClose={() => setDetailsProfile(null)}
          onSetActive={(id) => {
            setDetailsProfile(null);
            void setActive(id);
          }}
        />
      </CardContent>
    </Card>
  );
}
