import { act, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Subscription } from "@mglaroute/contracts";

vi.mock("../lib/invoke", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { listen } from "@tauri-apps/api/event";
import App from "../App";
import { invoke } from "../lib/invoke";
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

describe("app store backend events", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({
      subscriptions: [],
      serviceProfiles: [],
      profiles: [],
      settings: null,
      probeTested: 0,
      probeTotal: 0,
      lastProbeFinishedAt: null,
      notice: null,
    });
    let subscriptionListCalls = 0;
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_list") {
        subscriptionListCalls += 1;
        return (subscriptionListCalls === 1 ? [activeFeed] : [errorFeed]) as never;
      }
      if (command === "profile_list") return [] as never;
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
});
