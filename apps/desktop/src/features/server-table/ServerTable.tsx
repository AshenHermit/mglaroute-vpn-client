import type { Profile } from "@mglaroute/contracts";
import { Power } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeaderCell,
  TableRow,
} from "../../components/ui/table";

type ServerTableProps = {
  profiles: Profile[];
  /** Ids currently being probed, supplied by the probe UI. */
  testingIds?: string[];
  /** Session-only active target; drives the next Connect while disconnected. */
  selectedId?: string | null;
  /** Actual connected server from backend state; wins over selectedId. */
  connectedId?: string | null;
  onSelect?: (id: string) => void;
};

function isFailed(profile: Profile): boolean {
  return profile.latencyMs === null || profile.latencyMs <= 0 || !profile.reachable;
}

/** Pure presentation: eligibility and reconnect decisions stay in Rust. */
export function ServerTable({
  profiles,
  testingIds,
  selectedId,
  connectedId,
  onSelect,
}: ServerTableProps) {
  const testing = new Set(testingIds ?? []);
  const activeId = connectedId ?? selectedId;
  const sorted = [...profiles].sort((left, right) => {
    const leftFailed = isFailed(left);
    const rightFailed = isFailed(right);
    if (leftFailed !== rightFailed) return leftFailed ? 1 : -1;
    if (leftFailed && rightFailed) return left.sortIndex - right.sortIndex;
    const byDelay = (left.latencyMs ?? 0) - (right.latencyMs ?? 0);
    return byDelay !== 0 ? byDelay : left.sortIndex - right.sortIndex;
  });

  return (
    <div className="w-full min-w-0 overflow-x-auto">
      <Table className="table-fixed">
        <TableHead>
          <TableRow>
            <TableHeaderCell className="w-10" />
            <TableHeaderCell className="w-2/5">Server</TableHeaderCell>
            <TableHeaderCell>Protocol</TableHeaderCell>
            <TableHeaderCell>Address</TableHeaderCell>
            <TableHeaderCell>Delay</TableHeaderCell>
            <TableHeaderCell>State</TableHeaderCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {sorted.length === 0 ? (
            <TableRow>
              <TableCell colSpan={6} className="text-slate-500">
                No servers yet. Import a subscription and run a test.
              </TableCell>
            </TableRow>
          ) : (
            sorted.map((profile) => {
              const failed = isFailed(profile);
              const isTesting = testing.has(profile.id);
              const isConnected = connectedId === profile.id;
              const isActive = activeId === profile.id;
              const state = isConnected ? "connected" : isActive ? "active" : "inactive";
              const name = profile.remarks ?? profile.serverId;
              const label = isConnected
                ? `Connected server: ${name}`
                : isActive
                  ? `Active server: ${name}`
                  : `Set active server: ${name}`;
              const buttonClass = isConnected
                ? "border border-emerald-600 bg-emerald-600 text-white"
                : isActive
                  ? "border border-slate-400 bg-slate-400 text-white"
                  : "border border-slate-300 bg-white text-slate-700";
              return (
                <TableRow
                  key={profile.id}
                  data-testid="server-row"
                  data-profile-id={profile.id}
                  data-active={isActive ? "true" : "false"}
                  className={`${isActive ? "bg-slate-100" : ""} ${
                    failed ? "opacity-60 text-gray-500" : ""
                  }`}
                >
                  <TableCell>
                    <button
                      type="button"
                      data-testid="server-power"
                      data-profile-id={profile.id}
                      data-state={state}
                      aria-label={label}
                      onClick={() => onSelect?.(profile.id)}
                      className={`inline-flex h-7 w-7 items-center justify-center rounded ${buttonClass}`}
                    >
                      <Power aria-hidden="true" className="h-4 w-4" />
                    </button>
                  </TableCell>
                  <TableCell className="break-words">{name}</TableCell>
                  <TableCell className="break-words">{profile.protocol ?? "—"}</TableCell>
                  <TableCell className="break-words">{profile.address ?? "—"}</TableCell>
                  <TableCell>
                    {isTesting ? "…" : failed ? "—" : `${profile.latencyMs} ms`}
                  </TableCell>
                  <TableCell>
                    {isTesting ? "Testing..." : failed ? "failed" : "ok"}
                  </TableCell>
                </TableRow>
              );
            })
          )}
        </TableBody>
      </Table>
    </div>
  );
}
