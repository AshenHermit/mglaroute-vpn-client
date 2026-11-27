import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EVENTS, type Profile } from "@mglaroute/contracts";

vi.mock("../../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { CommandInvocationError, invoke } from "../../lib/invoke";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../../stores/app.store";
import { disconnected, useConnectionStore } from "../../stores/connection.store";
import { ConnectionPanel } from "./ConnectionPanel";

const mockedInvoke = vi.mocked(invoke);
const mockedListen = vi.mocked(listen);

function makeProfile(id: string, remarks: string, address: string): Profile {
  return {
    id,
    subscriptionId: "sub-1",
    serverId: id,
    link: "vless://secret@node.example:443",
    remarks,
    protocol: "vless",
    address,
    port: 443,
    rawConfig: '{"secret":true}',
    present: true,
    latencyMs: 42,
    reachable: true,
    checkedAt: null,
    sortIndex: 0,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  };
}

describe("ConnectionPanel", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
  beforeEach(() => {
    vi.clearAllMocks();
    // clearAllMocks leaves queued `mockResolvedValueOnce` values behind; drop
    // them so one case cannot feed the next case's `connection_status` call.
    mockedInvoke.mockReset();
    useConnectionStore.getState().setState(disconnected);
    useAppStore.getState().setNotice(null);
    useAppStore.getState().setSelectedProfileId(null);
    useAppStore.getState().setProfiles([]);
    mockedInvoke.mockResolvedValue({
      status: "disconnected",
      profileId: null,
      proxyPort: null,
    });
  });

  it("connects with no selected profile and renders connection-state events", async () => {
    render(<ConnectionPanel />);

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_status",
        expect.anything(),
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_connect",
        expect.anything(),
        { profileId: null },
      ),
    );

    const handler = mockedListen.mock.calls.find(
      ([event]) => event === EVENTS.connectionState,
    )?.[1];
    expect(handler).toBeTypeOf("function");
    act(() => {
      handler?.({
        payload: { status: "connected", profileId: "p-1", proxyPort: 10808 },
      } as never);
    });

    expect(screen.getByText("Connected")).toBeTruthy();
    expect(screen.getByText(/10808/)).toBeTruthy();
  });

  it("passes the selected profile id to connect", async () => {
    useAppStore.getState().setSelectedProfileId("p-2");
    render(<ConnectionPanel />);

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_status",
        expect.anything(),
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_connect",
        expect.anything(),
        { profileId: "p-2" },
      ),
    );
  });

  it("keeps state intact when connect is rejected as connection_busy", async () => {
    const { getByRole, queryByText, getByTestId } = render(<ConnectionPanel />);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_status",
        expect.anything(),
      ),
    );

    mockedInvoke.mockRejectedValueOnce(
      new CommandInvocationError("connection_connect", "connection_busy"),
    );
    fireEvent.click(getByRole("button", { name: "Connect" }));

    await waitFor(() =>
      expect(useAppStore.getState().notice).toBe(
        "Connection operation in progress",
      ),
    );
    expect(queryByText("Connection faulted. Reconnect to continue.")).toBeNull();
    expect(getByTestId("connection-status").textContent).toBe("Disconnected");
  });

  it("shows a clear port-in-use notice without entering faulted state", async () => {
    const { getByRole, queryByText, getByTestId } = render(<ConnectionPanel />);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_status",
        expect.anything(),
      ),
    );

    mockedInvoke.mockRejectedValueOnce(
      new CommandInvocationError("connection_connect", "port_in_use"),
    );
    fireEvent.click(getByRole("button", { name: "Connect" }));

    await waitFor(() =>
      expect(useAppStore.getState().notice).toBe(
        "Proxy port is in use by another program. Change it in Settings or stop that program, then reconnect.",
      ),
    );
    expect(queryByText("Connection faulted. Reconnect to continue.")).toBeNull();
    expect(getByTestId("connection-status").textContent).toBe("Disconnected");
  });

  it("keeps the faulted alert and the Reconnect name", async () => {
    mockedInvoke.mockResolvedValueOnce({
      status: "faulted",
      profileId: "p-1",
      proxyPort: 10808,
    });
    render(<ConnectionPanel />);

    expect((await screen.findByRole("alert")).textContent).toBe(
      "Connection faulted. Reconnect to continue.",
    );
    const power = screen.getByRole("button", { name: "Reconnect" });
    fireEvent.click(power);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_connect",
        expect.anything(),
        { profileId: null },
      ),
    );
  });

  it("disconnects through the power button and keeps the busy notice", async () => {
    mockedInvoke.mockResolvedValueOnce({
      status: "connected",
      profileId: "p-1",
      proxyPort: 10808,
    });
    render(<ConnectionPanel />);

    mockedInvoke.mockRejectedValueOnce(
      new CommandInvocationError("connection_disconnect", "connection_busy"),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Disconnect" }));

    await waitFor(() =>
      expect(useAppStore.getState().notice).toBe("Connection operation in progress"),
    );
    expect(mockedInvoke).toHaveBeenCalledWith(
      "connection_disconnect",
      expect.anything(),
    );
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
  });

  it("keeps Disconnect available while a backend-driven connection is in flight", async () => {
    mockedInvoke.mockResolvedValueOnce({
      status: "connecting",
      profileId: "p-1",
      proxyPort: null,
    });
    render(<ConnectionPanel />);

    const power = await screen.findByRole("button", { name: "Disconnect" });
    expect((power as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(power);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_disconnect",
        expect.anything(),
      ),
    );
  });

  it("shows the green ring and the connected server while connected", async () => {
    act(() => {
      useAppStore.setState({
        profiles: [makeProfile("p-1", "node 1", "node.example.com")],
      });
    });
    mockedInvoke.mockResolvedValueOnce({
      status: "connected",
      profileId: "p-1",
      proxyPort: 10808,
    });
    render(<ConnectionPanel />);

    const power = await screen.findByRole("button", { name: "Disconnect" });
    expect(power.className).toContain("ring-2");
    expect(power.className).toContain("ring-primary");

    const active = screen.getByTestId("active-server");
    expect(active.getAttribute("data-state")).toBe("connected");
    expect(active.textContent).toContain("node 1");
    expect(active.textContent).toContain("node.example.com");

    act(() => {
      useConnectionStore.getState().setState(disconnected);
    });
    expect(screen.getByRole("button", { name: "Connect" }).className).not.toContain(
      "ring-primary",
    );
  });

  it("shows the selected profile's remarks and address on active-server", async () => {
    act(() => {
      useAppStore.setState({
        profiles: [makeProfile("p-2", "Tokyo node", "203.0.113.7")],
        selectedProfileId: "p-2",
      });
    });
    render(<ConnectionPanel />);

    const active = await screen.findByTestId("active-server");
    expect(active.getAttribute("data-state")).toBe("active");
    expect(active.textContent).toContain("Tokyo node");
    expect(active.textContent).toContain("203.0.113.7");
    expect(active.textContent).not.toContain("vless://");
    expect(active.textContent).not.toContain("rawConfig");
  });

  it("shows the connected server id when it is missing from the profile list", async () => {
    render(<ConnectionPanel />);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_status",
        expect.anything(),
      ),
    );
    const handler = mockedListen.mock.calls.find(
      ([event]) => event === EVENTS.connectionState,
    )?.[1];
    expect(handler).toBeTypeOf("function");

    act(() => {
      handler?.({
        payload: { status: "connected", profileId: "p-gone", proxyPort: 10808 },
      } as never);
    });

    const active = screen.getByTestId("active-server");
    expect(active.getAttribute("data-state")).toBe("connected");
    expect(active.textContent).toContain("Connected server (not in list)");
    expect(active.textContent).toContain("p-gone");
    expect(active.textContent).not.toContain("No active server");
  });

  it("starts the timer at 00:00 on connect and resets after a disconnect event", async () => {
    render(<ConnectionPanel />);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "connection_status",
        expect.anything(),
      ),
    );
    const handler = mockedListen.mock.calls.find(
      ([event]) => event === EVENTS.connectionState,
    )?.[1];
    expect(handler).toBeTypeOf("function");

    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    act(() => {
      handler?.({
        payload: { status: "connected", profileId: "p-1", proxyPort: 10808 },
      } as never);
    });

    expect(screen.getByTestId("connection-timer").textContent).toBe("00:00");
    act(() => {
      vi.advanceTimersByTime(2500);
    });
    expect(screen.getByTestId("connection-timer").textContent).toBe("00:02");

    act(() => {
      handler?.({
        payload: { status: "disconnected", profileId: null, proxyPort: null },
      } as never);
    });
    expect(screen.getByTestId("connection-timer").textContent).toBe("00:00");
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByTestId("connection-timer").textContent).toBe("00:00");
  });
});
