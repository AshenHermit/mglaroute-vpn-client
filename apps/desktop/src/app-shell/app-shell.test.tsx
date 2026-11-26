import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Profile, Subscription } from "@mglaroute/contracts";

vi.mock("../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { disconnected, useConnectionStore } from "../stores/connection.store";
import { useUiStore } from "../stores/ui.store";
import { AppShell } from "./AppShell";

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

describe("AppShell", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    act(() => {
      useAppStore.setState({
        subscriptions: [],
        profiles: [],
        notice: null,
        probeTesting: false,
        probeTestingIds: [],
        probeTested: 0,
        probeTotal: 0,
        lastProbeFinishedAt: null,
        selectedProfileId: null,
      });
      useUiStore.setState({ favorites: [], filter: { kind: "all" } });
      useConnectionStore.getState().setState(disconnected);
    });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_list") return subscriptions as never;
      if (command === "profile_list") return profiles as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      if (command === "connection_status") return disconnected as never;
      return undefined as never;
    });
  });

  it("renders the brand and the three real filters plus the subscription group label", () => {
    render(<AppShell />);

    expect(screen.getByRole("heading", { name: "mglaroute" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "All servers" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Favorites" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reachable only" })).toBeTruthy();
    expect(screen.getByText("By subscription")).toBeTruthy();
  });

  it("mounts the connection panel with the connected server inside the right zone", async () => {
    render(<AppShell />);

    const rightZone = screen.getByTestId("right-zone");
    await waitFor(() =>
      expect(within(rightZone).getByTestId("connection-status").textContent).toBe(
        "Disconnected",
      ),
    );
    expect(
      within(rightZone).getByTestId("active-server").getAttribute("data-state"),
    ).toBe("none");
    expect(screen.queryByTestId("right-placeholder")).toBeNull();

    act(() => {
      useAppStore.setState({ profiles });
      useConnectionStore.getState().setState({
        status: "connected",
        profileId: "p-1",
        proxyPort: 10808,
      });
    });

    expect(within(rightZone).getByTestId("connection-status").textContent).toBe(
      "Connected",
    );
    const active = within(rightZone).getByTestId("active-server");
    expect(active.getAttribute("data-state")).toBe("connected");
    expect(active.textContent).toContain("node 1");
  });

  it("sets the favorites filter from the rail", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByRole("button", { name: "Favorites" }));

    expect(useUiStore.getState().filter).toEqual({ kind: "favorites" });
  });

  it("filters by opaque subscription id only, never by URL or raw config", async () => {
    render(<AppShell />);

    fireEvent.click(await screen.findByRole("button", { name: "Feed one" }));

    expect(useUiStore.getState().filter).toEqual({
      kind: "subscription",
      subscriptionId: "sub-1",
    });
    const persisted = window.localStorage.getItem("mglaroute-ui") ?? "";
    expect(persisted).not.toContain("https://secret.example");
    expect(persisted).not.toContain("rawConfig");
  });

  it("hydrates counts from subscription_list and profile_list without rendering secrets", async () => {
    render(<AppShell />);

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_list", expect.anything()),
    );
    expect(mockedInvoke).toHaveBeenCalledWith("profile_list", expect.anything());
    await waitFor(() =>
      expect(screen.getByTestId("subscription-count").textContent).toBe("2"),
    );
    expect(screen.getByTestId("server-count").textContent).toBe("3");
    expect(screen.queryByText(/secret\.example/)).toBeNull();
    expect(screen.queryByText(/vless:\/\//)).toBeNull();
  });

  it("shows the real anti-DPI status fetched once", async () => {
    render(<AppShell />);

    await waitFor(() =>
      expect(screen.getByTestId("anti-dpi-status").textContent).toContain("unsupported"),
    );
    expect(mockedInvoke).toHaveBeenCalledWith("anti_dpi_status", expect.anything());
    expect(
      mockedInvoke.mock.calls.filter(([command]) => command === "anti_dpi_status"),
    ).toHaveLength(1);
  });

  it("falls back to unsupported when anti_dpi_status fails, without leaking native text", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "anti_dpi_status") {
        throw new Error("native failure echoing https://secret.example");
      }
      if (command === "subscription_list") return subscriptions as never;
      if (command === "profile_list") return profiles as never;
      if (command === "connection_status") return disconnected as never;
      return undefined as never;
    });

    render(<AppShell />);

    await waitFor(() =>
      expect(screen.getByTestId("anti-dpi-status").textContent).toContain("unsupported"),
    );
    expect(screen.queryByText(/secret\.example/)).toBeNull();
  });

  it("toggles the gear placeholder from the gear button", () => {
    render(<AppShell />);

    expect(screen.queryByTestId("gear-placeholder")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
    expect(screen.getByTestId("gear-placeholder")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close settings" }));
    expect(screen.queryByTestId("gear-placeholder")).toBeNull();
  });
});
