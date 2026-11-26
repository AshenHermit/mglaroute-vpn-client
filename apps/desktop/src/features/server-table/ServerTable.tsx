import type { Profile } from "@mglaroute/contracts";
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
  /** Session-only selection; drives the next Connect. */
  selectedId?: string | null;
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
  onSelect,
}: ServerTableProps) {
  const testing = new Set(testingIds ?? []);
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
              <TableCell colSpan={5} className="text-slate-500">
                No servers yet. Import a subscription and run a test.
              </TableCell>
            </TableRow>
          ) : (
            sorted.map((profile) => {
              const failed = isFailed(profile);
              const isTesting = testing.has(profile.id);
              const isSelected = selectedId === profile.id;
              return (
                <TableRow
                  key={profile.id}
                  data-testid="server-row"
                  data-profile-id={profile.id}
                  data-selected={isSelected ? "true" : "false"}
                  role="button"
                  tabIndex={0}
                  aria-pressed={isSelected}
                  onClick={() => onSelect?.(profile.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onSelect?.(profile.id);
                    }
                  }}
                  className={`cursor-pointer ${isSelected ? "bg-slate-100" : ""} ${
                    failed ? "opacity-60 text-gray-500" : ""
                  }`}
                >
                  <TableCell className="break-words">
                    {profile.remarks ?? profile.serverId}
                  </TableCell>
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
