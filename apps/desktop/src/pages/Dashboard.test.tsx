import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionState, Profile, Subscription } from "@mglaroute/contracts";

vi.mock("../lib/invoke", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { useConnectionStore } from "../stores/connection.store";
import Dashboard from "./Dashboard";

const mockedInvoke = vi.mocked(invoke);
const disconnected: ConnectionState = {
  status: "disconnected",
  profileId: null,
  proxyPort: null,
};

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
  afterEach(cleanup);

  let connectionState: ConnectionState = disconnected;

  beforeEach(() => {
    vi.clearAllMocks();
    connectionState = disconnected;
    act(() => {
      useAppStore.setState({
        probeTesting: false,
        probeTestingIds: [],
        probeCancelled: false,
        notice: null,
        selectedProfileId: null,
      });
      useConnectionStore.setState({ state: disconnected });
    });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_list") return subscriptions as never;
      if (command === "profile_list") return profiles as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      if (command === "connection_status") return connectionState as never;
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

  it("dashboard_shows_live_testing_state", async () => {
    act(() => {
      useAppStore.setState({
        probeTesting: true,
        probeTestingIds: ["p-0"],
        probeTested: 1,
        probeTotal: 3,
      });
    });
    render(<Dashboard />);

    expect(screen.getByText("Testing...")).toBeTruthy();
    expect(screen.queryByText("No test cycle yet.")).toBeNull();
  });

  it("dashboard shows the connected server", async () => {
    connectionState = { status: "connected", profileId: "p-1", proxyPort: 10809 };
    render(<Dashboard />);

    const active = await screen.findByTestId("active-server");
    expect(active.getAttribute("data-state")).toBe("connected");
    expect(active.textContent).toContain("node 1");
  });

  it("dashboard shows a selected server when disconnected", async () => {
    act(() => {
      useAppStore.setState({ selectedProfileId: "p-2" });
    });
    render(<Dashboard />);

    const active = await screen.findByTestId("active-server");
    expect(active.getAttribute("data-state")).toBe("active");
    expect(active.textContent).toContain("node 2");
  });
});
