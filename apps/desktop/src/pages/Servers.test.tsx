import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Profile } from "@mglaroute/contracts";
import { CommandInvocationError } from "../lib/invoke";

// Keep the real error-code mapping; only the IPC transport is faked.
vi.mock("../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
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

describe("Servers probe controls", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({
      profiles: [server],
      probeTesting: false,
      probeTestingIds: [],
      probeCancelled: false,
      notice: null,
      selectedProfileId: null,
    });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "profile_list") return [server] as never;
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
});
