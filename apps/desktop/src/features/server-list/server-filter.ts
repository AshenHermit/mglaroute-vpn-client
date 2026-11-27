import type { Profile } from "@mglaroute/contracts";
import type { ServerFilter } from "../../stores/ui.store";

/**
 * Pure view filter. Sorting stays in `ServerTable`; no profile or secret is
 * mutated, persisted or logged here.
 */
export function filterProfiles(
  profiles: Profile[],
  filter: ServerFilter,
  favorites: string[],
): Profile[] {
  switch (filter.kind) {
    case "all":
      return profiles;
    case "favorites": {
      const ids = new Set(favorites);
      return profiles.filter((profile) => ids.has(profile.id));
    }
    case "subscription":
      return filter.subscriptionId
        ? profiles.filter((profile) => profile.subscriptionId === filter.subscriptionId)
        : [];
    case "reachable":
      return profiles.filter(
        (profile) =>
          profile.latencyMs !== null &&
          profile.latencyMs > 0 &&
          profile.reachable === true,
      );
  }
}
