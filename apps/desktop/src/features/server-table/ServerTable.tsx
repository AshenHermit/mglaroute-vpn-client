import type { Profile } from "@mglaroute/contracts";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeaderCell,
  TableRow,
} from "../../components/ui/table";

function isFailed(profile: Profile): boolean {
  return profile.latencyMs === null || profile.latencyMs <= 0 || !profile.reachable;
}

/** Pure presentation: eligibility and reconnect decisions stay in Rust. */
export function ServerTable({ profiles }: { profiles: Profile[] }) {
  const sorted = [...profiles].sort((left, right) => {
    const leftFailed = isFailed(left);
    const rightFailed = isFailed(right);
    if (leftFailed !== rightFailed) return leftFailed ? 1 : -1;
    if (leftFailed && rightFailed) return left.sortIndex - right.sortIndex;
    const byDelay = (left.latencyMs ?? 0) - (right.latencyMs ?? 0);
    return byDelay !== 0 ? byDelay : left.sortIndex - right.sortIndex;
  });

  return (
    <Table>
      <TableHead>
        <TableRow>
          <TableHeaderCell>Server</TableHeaderCell>
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
            return (
              <TableRow
                key={profile.id}
                data-testid="server-row"
                data-profile-id={profile.id}
                className={failed ? "opacity-60 text-gray-500" : ""}
              >
                <TableCell>{profile.remarks ?? profile.serverId}</TableCell>
                <TableCell>{profile.protocol ?? "—"}</TableCell>
                <TableCell>{profile.address ?? "—"}</TableCell>
                <TableCell>
                  {failed ? "—" : `${profile.latencyMs} ms`}
                </TableCell>
                <TableCell>{failed ? "failed" : "ok"}</TableCell>
              </TableRow>
            );
          })
        )}
      </TableBody>
    </Table>
  );
}
