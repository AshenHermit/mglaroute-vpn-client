import type { Profile } from "@mglaroute/contracts";
import { describe, expect, it } from "vitest";
import { filterProfiles } from "./server-filter";

function profile(overrides: Partial<Profile> = {}): Profile {
  return {
    id: "p-1",
    subscriptionId: "sub-1",
    serverId: "p-1",
    link: "vless://example",
    remarks: "node 1",
    protocol: "vless",
    address: "node.example.com",
    port: 443,
    rawConfig: "{}",
    present: true,
    latencyMs: 10,
    reachable: true,
    checkedAt: null,
    sortIndex: 0,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
    ...overrides,
  };
}

function ids(profiles: Profile[]): string[] {
  return profiles.map((item) => item.id);
}

describe("filterProfiles", () => {
  it("all returns the input unchanged", () => {
    const profiles = [profile(), profile({ id: "p-2" })];
    expect(filterProfiles(profiles, { kind: "all" }, [])).toBe(profiles);
  });

  it("favorites returns only ids in the favorites array", () => {
    const profiles = [
      profile(),
      profile({ id: "p-2", subscriptionId: "sub-2" }),
      profile({ id: "p-3" }),
    ];
    const result = filterProfiles(profiles, { kind: "favorites" }, ["p-2", "p-3"]);
    expect(ids(result)).toEqual(["p-2", "p-3"]);
  });

  it("subscription returns only matching subscriptionId profiles", () => {
    const profiles = [
      profile(),
      profile({ id: "p-2", subscriptionId: "sub-2" }),
      profile({ id: "p-3", subscriptionId: "sub-2" }),
    ];
    const result = filterProfiles(
      profiles,
      { kind: "subscription", subscriptionId: "sub-2" },
      [],
    );
    expect(ids(result)).toEqual(["p-2", "p-3"]);
  });

  it("reachable keeps only positive latency and reachable profiles", () => {
    const profiles = [
      profile({ id: "p-fast", latencyMs: 10, reachable: true }),
      profile({ id: "p-slow", latencyMs: 0, reachable: true }),
      profile({ id: "p-negative", latencyMs: -1, reachable: true }),
      profile({ id: "p-unreachable", latencyMs: 12, reachable: false }),
      profile({ id: "p-untested", latencyMs: null, reachable: false }),
    ];
    expect(ids(filterProfiles(profiles, { kind: "reachable" }, []))).toEqual(["p-fast"]);
  });

  it("excludes a profile with null latency and reachable false", () => {
    const untested = profile({ latencyMs: null, reachable: false });
    expect(filterProfiles([untested], { kind: "reachable" }, [])).toEqual([]);
  });

  it("returns no profiles for an unknown subscription id", () => {
    const profiles = [profile(), profile({ id: "p-2" })];
    expect(
      filterProfiles(
        profiles,
        { kind: "subscription", subscriptionId: "sub-missing" },
        [],
      ),
    ).toEqual([]);
  });
});
