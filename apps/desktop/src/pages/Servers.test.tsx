import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConnectionStateSchema, type Profile } from "@mglaroute/contracts";
import { CommandInvocationError } from "../lib/invoke";

// Keep the real error-code mapping; only the IPC transport is faked.
vi.mock("../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { useConnectionStore } from "../stores/connection.store";
import { useUiStore } from "../stores/ui.store";
import Servers from "./Servers";

const mockedInvoke = vi.mocked(invoke);

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

const otherServer: Profile = {
  ...server,
  id: "p-2",
  serverId: "p-2",
  remarks: "node 2",
  sortIndex: 1,
};

describe("Servers probe controls", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    useConnectionStore.setState({
      state: { status: "disconnected", profileId: null, proxyPort: null },
    });
    useAppStore.setState({
      profiles: [server, otherServer],
      probeTested: 0,
      probeTotal: 0,
      probeTesting: false,
      probeTestingIds: [],
      probeCancelled: false,
      lastProbeFinishedAt: null,
      notice: null,
      selectedProfileId: null,
    });
    useUiStore.setState({ favorites: [], filter: { kind: "all" } });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer] as never;
      if (command === "probe_run") {
        throw new CommandInvocationError("probe_run", "probe_busy");
      }
      return undefined as never;
    });
  });

  it("maps probe_busy from probe_run to the in-progress notice", async () => {
    render(<Servers />);
    fireEvent.click(screen.getByRole("button", { name: "Run test" }));
    await waitFor(() =>
      expect(useAppStore.getState().notice).toBe(
        "Another operation is in progress. Try again shortly.",
      ),
    );
  });

  it("clears a stale probeCancelled when a new run begins", async () => {
    useAppStore.setState({ probeCancelled: true });
    render(<Servers />);
    fireEvent.click(screen.getByRole("button", { name: "Run test" }));
    await waitFor(() => expect(useAppStore.getState().probeCancelled).toBe(false));
  });

  it("shows a spinner while servers are being tested", () => {
    useAppStore.setState({ probeTesting: true, probeTestingIds: ["p-1"] });
    render(<Servers />);
    const [button] = screen.getAllByRole("button", { name: /Testing/ });
    expect(button.querySelector(".animate-spin")).not.toBeNull();
  });

  it("shows testing state on click before any backend event arrives", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer] as never;
      // Never settles: the click must update the UI synchronously.
      return await new Promise(() => undefined);
    });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));

    fireEvent.click(screen.getByRole("button", { name: "Run test" }));

    expect(useAppStore.getState().probeTesting).toBe(true);
    expect(useAppStore.getState().probeTestingIds).toEqual([]);
    expect(screen.getByRole("button", { name: "Stop test" })).not.toBeNull();
    expect(
      screen.getAllByTestId("server-row").some((row) =>
        row.textContent?.includes("Testing..."),
      ),
    ).toBe(false);
  });

  it("highlights only the profile from a testing event", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer] as never;
      return await new Promise(() => undefined);
    });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));

    act(() => {
      useAppStore.getState().setProbeServer({
        profileId: "p-2",
        status: "testing",
        latencyMs: null,
        serviceResults: [],
      });
    });

    expect(useAppStore.getState().probeTestingIds).toEqual(["p-2"]);
    const rows = screen.getAllByTestId("server-row");
    const highlighted = rows.filter((row) => row.textContent?.includes("Testing..."));
    expect(highlighted).toHaveLength(1);
    expect(highlighted[0].getAttribute("data-profile-id")).toBe("p-2");
  });

  it("clears testing state when probe_run fails", async () => {
    render(<Servers />);
    fireEvent.click(screen.getByRole("button", { name: "Run test" }));

    await waitFor(() =>
      expect(useAppStore.getState().probeTesting).toBe(false),
    );
    expect(useAppStore.getState().probeTestingIds).toEqual([]);
    expect(screen.queryByRole("button", { name: "Stop test" })).toBeNull();
  });

  it("sets the active server without connecting when disconnected", async () => {
    useConnectionStore.setState({
      state: { status: "disconnected", profileId: null, proxyPort: null },
    });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer] as never;
      return await new Promise(() => undefined);
    });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));
    const p2 = screen.getAllByTestId("server-power").find(
      (button) => button.getAttribute("data-profile-id") === "p-2",
    );
    fireEvent.click(p2!);
    expect(useAppStore.getState().selectedProfileId).toBe("p-2");
    expect(mockedInvoke).not.toHaveBeenCalledWith(
      "connection_connect",
      expect.anything(),
      expect.anything(),
    );
  });

  it("switches the live connection when power is clicked while connected", async () => {
    useConnectionStore.setState({
      state: { status: "connected", profileId: "p-1", proxyPort: 10809 },
    });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer] as never;
      if (command === "connection_connect") {
        return { status: "connected", profileId: "p-2", proxyPort: 10809 } as never;
      }
      return await new Promise(() => undefined);
    });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));
    const p2 = screen.getAllByTestId("server-power").find(
      (button) => button.getAttribute("data-profile-id") === "p-2",
    );
    fireEvent.click(p2!);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_connect",
        ConnectionStateSchema,
        { profileId: "p-2" },
      ),
    );
    expect(useAppStore.getState().selectedProfileId).toBe("p-2");
  });

  it("keeps the old active server and reports a busy switch", async () => {
    useConnectionStore.setState({
      state: { status: "connected", profileId: "p-1", proxyPort: 10809 },
    });
    useAppStore.setState({ selectedProfileId: "p-1" });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer] as never;
      if (command === "connection_connect") {
        throw new CommandInvocationError("connection_connect", "connection_busy");
      }
      return undefined as never;
    });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));
    const p2 = screen.getAllByTestId("server-power").find(
      (button) => button.getAttribute("data-profile-id") === "p-2",
    );
    fireEvent.click(p2!);

    await waitFor(() =>
      expect(useAppStore.getState().notice).toContain("busy"),
    );
    expect(useAppStore.getState().selectedProfileId).toBe("p-1");
  });

  it("narrows rows by a case-insensitive search over remarks", async () => {
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));

    fireEvent.change(screen.getByRole("searchbox", { name: "Search servers" }), {
      target: { value: "NODE 2" },
    });

    const rows = screen.getAllByTestId("server-row");
    expect(rows).toHaveLength(1);
    expect(rows[0].getAttribute("data-profile-id")).toBe("p-2");
  });

  it("searches address and protocol as well as remarks", async () => {
    const third: Profile = {
      ...server,
      id: "p-3",
      serverId: "p-3",
      remarks: "node 3",
      protocol: "trojan",
      address: "edge.example.net",
      sortIndex: 2,
    };
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server, otherServer, third] as never;
      return undefined as never;
    });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(3));

    const search = screen.getByRole("searchbox", { name: "Search servers" });
    fireEvent.change(search, { target: { value: "edge.example" } });
    expect(screen.getAllByTestId("server-row")).toHaveLength(1);
    expect(screen.getByTestId("server-row").getAttribute("data-profile-id")).toBe("p-3");

    fireEvent.change(search, { target: { value: "VLESS" } });
    expect(screen.getAllByTestId("server-row")).toHaveLength(2);
  });

  it("shows only favorited rows when the favorites filter is active", async () => {
    useUiStore.setState({ favorites: ["p-2"] });
    useUiStore.getState().setFilter({ kind: "favorites" });
    render(<Servers />);
    await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));

    const rows = screen.getAllByTestId("server-row");
    expect(rows).toHaveLength(1);
    expect(rows[0].getAttribute("data-profile-id")).toBe("p-2");
  });

  it("renders probe progress, Stop test and the finished timestamp from the store", () => {
    useAppStore.setState({
      probeTesting: true,
      probeTested: 3,
      probeTotal: 5,
      lastProbeFinishedAt: "2026-11-26T10:00:00Z",
    });
    render(<Servers />);

    expect(screen.getByText("3 / 5")).toBeTruthy();
    expect(screen.getByText("Finished: 26.11.2026 13:00:00")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop test" })).toBeTruthy();
  });
});
