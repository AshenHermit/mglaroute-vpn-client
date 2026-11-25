import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { EVENTS } from "@mglaroute/contracts";

vi.mock("../../lib/invoke", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { invoke } from "../../lib/invoke";
import { listen } from "@tauri-apps/api/event";
import { ConnectionPanel } from "./ConnectionPanel";

const mockedInvoke = vi.mocked(invoke);
const mockedListen = vi.mocked(listen);

describe("ConnectionPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
});
