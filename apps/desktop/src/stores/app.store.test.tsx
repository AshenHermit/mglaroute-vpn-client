import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Profile, Subscription } from "@mglaroute/contracts";
import { z } from "zod";

vi.mock("../lib/invoke", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { listen } from "@tauri-apps/api/event";
import App from "../App";
import { invoke } from "../lib/invoke";
import Servers from "../pages/Servers";
import { useAppStore } from "./app.store";

const mockedInvoke = vi.mocked(invoke);
const mockedListen = vi.mocked(listen);

function subscription(id: string, status: Subscription["status"]): Subscription {
  return {
    id,
    name: `Feed ${id}`,
    url: `https://secret.example/${id}`,
    enabled: true,
    status,
    autoRefetchMinutes: 60,
    lastFetchedAt: null,
    lastError: null,
    userAgent: null,
    requestHeaders: null,
    sort: 0,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  };
}

const activeFeed = subscription("sub-1", "active");
const errorFeed = subscription("sub-1", "error");

const server: Profile = {
  id: "p-1",
  subscriptionId: "sub-1",
  serverId: "p-1",
  link: "vless://secret@node.example.com:443",
  remarks: "node 1",
  protocol: "vless",
  address: "node.example.com",
  port: 443,
  rawConfig: "{}",
  present: true,
  latencyMs: null,
  reachable: false,
  checkedAt: null,
  sortIndex: 0,
  createdAt: "2026-11-24T00:00:00Z",
  updatedAt: "2026-11-24T00:00:00Z",
};

function eventHandler(event: string) {
  return mockedListen.mock.calls.find(([name]) => name === event)?.[1];
}

describe("app store backend events", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({
      subscriptions: [],
      serviceProfiles: [],
      profiles: [server],
      settings: null,
      probeTested: 0,
      probeTotal: 0,
      lastProbeFinishedAt: null,
      probeTesting: false,
      probeTestingIds: [],
      probeCancelled: false,
      notice: null,
      selectedProfileId: null,
    });
    let subscriptionListCalls = 0;
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_list") {
        subscriptionListCalls += 1;
        return (subscriptionListCalls === 1 ? [activeFeed] : [errorFeed]) as never;
      }
      if (command === "profile_list") return [server] as never;
      if (command === "probe_cancel") return true as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      if (command === "connection_status") {
        return { status: "disconnected", profileId: null, proxyPort: null } as never;
      }
      throw new Error(`unexpected command ${command}`);
    });
  });

  it("subscription_refreshed_event_reloads_list", async () => {
    render(<App />);

    await waitFor(() =>
      expect(
        mockedInvoke.mock.calls.filter(([command]) => command === "subscription_list"),
      ).toHaveLength(1),
    );
    expect(useAppStore.getState().subscriptions[0]?.status).toBe("active");

    const handler = mockedListen.mock.calls.find(
      ([event]) => event === "subscription-refreshed",
    )?.[1];
    expect(handler).toBeTypeOf("function");

    act(() => {
      handler?.({
        payload: {
          subscriptionId: "sub-1",
          added: 1,
          updated: 0,
          absent: 0,
          badLines: 0,
        },
      } as never);
    });

    await waitFor(() =>
      expect(
        mockedInvoke.mock.calls.filter(([command]) => command === "subscription_list"),
      ).toHaveLength(2),
    );
    await waitFor(() =>
      expect(useAppStore.getState().subscriptions[0]?.status).toBe("error"),
    );
  });

  it("shows per-server testing and disables Run test", async () => {
    act(() => {
      useAppStore.getState().setProbeServer({
        profileId: "p-1",
        status: "testing",
        latencyMs: null,
        serviceResults: [],
      });
    });
    render(<Servers />);

    expect(screen.getByTestId("server-row").textContent).toContain("Testing...");
    expect(
      (screen.getByRole("button", { name: "Testing..." }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("button", { name: "Stop test" }) as HTMLButtonElement).disabled,
    ).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "Stop test" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("probe_cancel", z.boolean()),
    );
  });

  it("tracks live server statuses and progress counts from probe events", () => {
    render(<App />);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "testing",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    act(() => {
      eventHandler("probe-progress")?.({ payload: { tested: 1, total: 2 } } as never);
    });

    expect(useAppStore.getState().probeTesting).toBe(true);
    expect(useAppStore.getState().probeTestingIds).toEqual(["p-1"]);
    expect(useAppStore.getState().probeTested).toBe(1);
    expect(useAppStore.getState().probeTotal).toBe(2);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "done",
          latencyMs: 12,
          serviceResults: [],
        },
      } as never);
    });
    expect(useAppStore.getState().probeTesting).toBe(false);
    expect(useAppStore.getState().probeTestingIds).toEqual([]);
    expect(useAppStore.getState().profiles[0]?.latencyMs).toBe(12);
    expect(useAppStore.getState().profiles[0]?.reachable).toBe(false);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "testing",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "cancelled",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    expect(useAppStore.getState().probeTesting).toBe(false);
    expect(useAppStore.getState().probeTestingIds).toEqual([]);
  });

  it("patches done and failed rows before probe_run settles", () => {
    render(<App />);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "testing",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "done",
          latencyMs: 50,
          serviceResults: [{ serviceProfileId: "default-google", reachable: true }],
        },
      } as never);
    });
    expect(useAppStore.getState().profiles[0]?.latencyMs).toBe(50);
    expect(useAppStore.getState().profiles[0]?.reachable).toBe(true);
    expect(useAppStore.getState().probeTestingIds).toEqual([]);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "failed",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    expect(useAppStore.getState().profiles[0]?.latencyMs).toBe(null);
    expect(useAppStore.getState().profiles[0]?.reachable).toBe(false);
  });

  it("cancelled removes highlight without overwriting the row", () => {
    render(<App />);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "done",
          latencyMs: 30,
          serviceResults: [{ serviceProfileId: "default-google", reachable: true }],
        },
      } as never);
    });
    expect(useAppStore.getState().profiles[0]?.latencyMs).toBe(30);
    expect(useAppStore.getState().profiles[0]?.reachable).toBe(true);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "testing",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    expect(useAppStore.getState().probeTestingIds).toEqual(["p-1"]);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "cancelled",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });
    expect(useAppStore.getState().probeTestingIds).toEqual([]);
    expect(useAppStore.getState().profiles[0]?.latencyMs).toBe(30);
    expect(useAppStore.getState().profiles[0]?.reachable).toBe(true);
  });

  it("reports cancellation and clears testing state on probe-finished", () => {
    act(() => {
      useAppStore.setState({
        probeTesting: true,
        probeTestingIds: ["p-1"],
        probeCancelled: false,
        notice: null,
      });
    });
    render(<App />);

    act(() => {
      eventHandler("probe-cancelled")?.({ payload: {} } as never);
    });
    expect(useAppStore.getState().probeCancelled).toBe(true);

    act(() => {
      eventHandler("probe-finished")?.({
        payload: {
          tested: 3,
          sorted: 0,
          finishedAt: "2026-11-25T00:00:00Z",
          cancelled: true,
        },
      } as never);
    });

    const state = useAppStore.getState();
    expect(state.probeTesting).toBe(false);
    expect(state.probeTestingIds).toEqual([]);
    expect(state.probeCancelled).toBe(true);
    expect(screen.getByRole("status").textContent).toContain("Test cancelled");
  });

  it("a new backend test run clears a stale probeCancelled", () => {
    act(() => {
      useAppStore.setState({
        probeCancelled: true,
        probeTesting: false,
        probeTestingIds: [],
      });
    });
    render(<App />);

    act(() => {
      eventHandler("probe-server")?.({
        payload: {
          profileId: "p-1",
          status: "testing",
          latencyMs: null,
          serviceResults: [],
        },
      } as never);
    });

    expect(useAppStore.getState().probeCancelled).toBe(false);
    expect(useAppStore.getState().probeTesting).toBe(true);
  });
});
