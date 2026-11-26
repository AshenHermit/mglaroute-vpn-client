import type { Profile } from "@mglaroute/contracts";
import { ChevronRight, Power, Star } from "lucide-react";
import { cn } from "../../lib/utils";
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
  /** UI-only favorites; never written back to a profile or an API payload. */
  favorites?: string[];
  onToggleFavorite?: (id: string) => void;
  onOpenDetails?: (profile: Profile) => void;
};

function isFailed(profile: Profile): boolean {
  return (
    profile.latencyMs === null || profile.latencyMs <= 0 || !profile.reachable
  );
}

const iconButtonClass =
  "inline-flex min-h-11 min-w-11 touch-manipulation items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background";

/** Pure presentation: eligibility and reconnect decisions stay in Rust. */
export function ServerTable({
  profiles,
  testingIds,
  selectedId,
  connectedId,
  onSelect,
  favorites = [],
  onToggleFavorite,
  onOpenDetails,
}: ServerTableProps) {
  const testing = new Set(testingIds ?? []);
  const activeId = connectedId ?? selectedId;
  const favoriteIds = new Set(favorites);
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
      <Table className="table-fixed min-w-[560px]">
        <colgroup>
          <col className="w-10" />
          <col className="w-[40%]" />
          <col className="w-[90px]" />
          <col className="w-[120px]" />
          <col className="w-[70px]" />
          <col className="w-[80px]" />
        </colgroup>
        <TableHead>
          <TableRow>
            <TableHeaderCell className="whitespace-nowrap" />
            <TableHeaderCell className="whitespace-nowrap">
              Server
            </TableHeaderCell>
            <TableHeaderCell className="hidden whitespace-nowrap sm:table-cell">
              Protocol
            </TableHeaderCell>
            <TableHeaderCell className="hidden whitespace-nowrap sm:table-cell">
              Address
            </TableHeaderCell>
            <TableHeaderCell className="whitespace-nowrap">
              Delay
            </TableHeaderCell>
            <TableHeaderCell className="whitespace-nowrap">
              State
            </TableHeaderCell>
          </TableRow>
        </TableHead>
        <TableBody className="overflow-y-auto overscroll-contain h-full">
          {sorted.length === 0 ? (
            <TableRow>
              <TableCell colSpan={6} className="text-muted-foreground">
                No servers yet. Import a subscription and run a test.
              </TableCell>
            </TableRow>
          ) : (
            sorted.map((profile) => {
              const failed = isFailed(profile);
              const isTesting = testing.has(profile.id);
              const isConnected = connectedId === profile.id;
              const isActive = activeId === profile.id;
              const isFavorite = favoriteIds.has(profile.id);
              const state = isConnected
                ? "connected"
                : isActive
                  ? "active"
                  : "inactive";
              const name = profile.remarks ?? profile.serverId;
              const label = isConnected
                ? `Connected server: ${name}`
                : isActive
                  ? `Active server: ${name}`
                  : `Set active server: ${name}`;
              const buttonClass = isConnected
                ? "border border-primary bg-primary text-primary-foreground"
                : isActive
                  ? "border border-muted-foreground bg-muted-foreground text-background"
                  : "border border-border bg-muted text-foreground";
              return (
                <TableRow
                  key={profile.id}
                  data-testid="server-row"
                  data-profile-id={profile.id}
                  data-active={isActive ? "true" : "false"}
                  className={`${isActive ? "bg-muted" : ""} ${
                    failed ? "opacity-60 text-muted-foreground" : ""
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
                      className={cn(
                        "inline-flex min-h-11 min-w-11 touch-manipulation items-center justify-center rounded focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                        buttonClass,
                      )}
                    >
                      <Power aria-hidden="true" className="h-4 w-4" />
                    </button>
                  </TableCell>
                  <TableCell className="truncate" title={name}>
                    <div className="flex items-center justify-between gap-2">
                      <span className="min-w-0 truncate" title={name}>
                        {name}
                      </span>
                      <span className="flex shrink-0 items-center gap-1">
                        <button
                          type="button"
                          data-testid="server-favorite"
                          aria-label={`${isFavorite ? "Remove" : "Add"} favorite: ${name}`}
                          aria-pressed={isFavorite}
                          onClick={() => onToggleFavorite?.(profile.id)}
                          className={iconButtonClass}
                        >
                          <Star
                            aria-hidden="true"
                            className="h-4 w-4"
                            fill={isFavorite ? "currentColor" : "none"}
                          />
                        </button>
                        <button
                          type="button"
                          data-testid="server-details"
                          aria-label={`Server details: ${name}`}
                          onClick={() => onOpenDetails?.(profile)}
                          className={iconButtonClass}
                        >
                          <ChevronRight
                            aria-hidden="true"
                            className="h-4 w-4"
                          />
                        </button>
                      </span>
                    </div>
                  </TableCell>
                  <TableCell
                    className="hidden truncate sm:table-cell"
                    title={profile.protocol ?? undefined}
                  >
                    {profile.protocol ?? "—"}
                  </TableCell>
                  <TableCell
                    className="hidden truncate sm:table-cell"
                    title={profile.address ?? undefined}
                  >
                    {profile.address ?? "—"}
                  </TableCell>
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
