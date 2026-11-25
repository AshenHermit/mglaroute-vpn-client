import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Profile, Subscription } from "@mglaroute/contracts";

vi.mock("../lib/invoke", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { invoke } from "../lib/invoke";
import Dashboard from "./Dashboard";

const mockedInvoke = vi.mocked(invoke);

const subscriptions: Subscription[] = [
  {
    id: "sub-1",
    name: "Feed one",
    url: "https://secret.example/1",
    enabled: true,
    status: "active",
    autoRefetchMinutes: 60,
    lastFetchedAt: null,
    lastError: null,
    userAgent: null,
    requestHeaders: null,
    sort: 0,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  },
  {
    id: "sub-2",
    name: "Feed two",
    url: "https://secret.example/2",
    enabled: true,
    status: "error",
    autoRefetchMinutes: 60,
    lastFetchedAt: null,
    lastError: null,
    userAgent: null,
    requestHeaders: null,
    sort: 1,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  },
];

const profiles: Profile[] = Array.from({ length: 3 }, (_, index) => ({
  id: `p-${index}`,
  subscriptionId: "sub-1",
  serverId: `p-${index}`,
  link: "vless://secret@node.example.com:443",
  remarks: `node ${index}`,
  protocol: "vless",
  address: "node.example.com",
  port: 443,
  rawConfig: "{}",
  present: true,
  latencyMs: null,
  reachable: false,
  checkedAt: null,
  sortIndex: index,
  createdAt: "2026-11-24T00:00:00Z",
  updatedAt: "2026-11-24T00:00:00Z",
}));

describe("Dashboard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_list") return subscriptions as never;
      if (command === "profile_list") return profiles as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      if (command === "connection_status") {
        return { status: "disconnected", profileId: null, proxyPort: null } as never;
      }
      throw new Error(`unexpected command ${command}`);
    });
  });

  it("dashboard_hydrates_counts_from_commands", async () => {
    render(<Dashboard />);

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "subscription_list",
        expect.anything(),
      ),
    );
    expect(mockedInvoke).toHaveBeenCalledWith("profile_list", expect.anything());

    expect((await screen.findAllByText("2")).length).toBeGreaterThan(0);
    expect((await screen.findAllByText("3")).length).toBeGreaterThan(0);
  });
});
