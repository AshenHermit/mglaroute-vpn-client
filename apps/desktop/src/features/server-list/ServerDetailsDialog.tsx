import type { Profile } from "@mglaroute/contracts";
import { Button } from "../../components/ui/button";
import { Dialog } from "../../components/ui/dialog";

export type ServerDetailsDialogProps = {
  profile: Profile | null;
  connected: boolean;
  onClose: () => void;
  onSetActive: (id: string) => void;
};

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-4">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="break-words text-right">{value}</dd>
    </div>
  );
}

/**
 * Read-only server facts from the profile already held in memory. Never render
 * the link, credentials or raw config, and never fabricate traffic or flags.
 */
export function ServerDetailsDialog({
  profile,
  connected,
  onClose,
  onSetActive,
}: ServerDetailsDialogProps) {
  if (!profile) return null;

  return (
    <Dialog open onClose={onClose} title="Server details">
      <dl className="grid gap-2 text-sm">
        <Fact label="Name" value={profile.remarks ?? profile.serverId} />
        <Fact label="Protocol" value={profile.protocol ?? "—"} />
        <Fact label="Address" value={profile.address ?? "—"} />
        <Fact label="Port" value={profile.port === null ? "—" : String(profile.port)} />
        <Fact
          label="Delay"
          value={profile.latencyMs === null ? "—" : `${profile.latencyMs} ms`}
        />
      </dl>
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          {connected && (
            <span className="rounded-full border border-primary/30 bg-primary/10 px-3 py-1 text-xs text-primary">
              Connected
            </span>
          )}
        </div>
        <Button onClick={() => onSetActive(profile.id)}>Set active</Button>
      </div>
    </Dialog>
  );
}
