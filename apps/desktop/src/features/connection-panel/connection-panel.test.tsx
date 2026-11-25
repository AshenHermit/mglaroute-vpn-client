import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EVENTS } from "@mglaroute/contracts";

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

describe("ConnectionPanel", () => {
  afterEach(() => cleanup());
  beforeEach(() => {
    vi.clearAllMocks();
    useConnectionStore.getState().setState(disconnected);
    useAppStore.getState().setNotice(null);
    useAppStore.getState().setSelectedProfileId(null);
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
});
