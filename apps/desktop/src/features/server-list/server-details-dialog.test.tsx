import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { Profile } from "@mglaroute/contracts";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ServerDetailsDialog } from "./ServerDetailsDialog";

const profile: Profile = {
  id: "p-1",
  subscriptionId: "sub-1",
  serverId: "p-1",
  link: "vless://example",
  remarks: "node 1",
  protocol: "vless",
  address: "node.example.com",
  port: 443,
  rawConfig: "{}",
  present: true,
  latencyMs: 42,
  reachable: true,
  checkedAt: null,
  sortIndex: 0,
  createdAt: "2026-11-24T00:00:00Z",
  updatedAt: "2026-11-24T00:00:00Z",
};

describe("ServerDetailsDialog", () => {
  afterEach(cleanup);

  it("renders server facts without revealing link or raw config", () => {
    render(
      <ServerDetailsDialog
        profile={profile}
        connected={false}
        onClose={vi.fn()}
        onSetActive={vi.fn()}
      />,
    );
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByText("node 1")).toBeTruthy();
    expect(dialog.getByText("vless")).toBeTruthy();
    expect(dialog.getByText("node.example.com")).toBeTruthy();
    expect(dialog.getByText("443")).toBeTruthy();
    expect(dialog.getByText("42 ms")).toBeTruthy();
    expect(dialog.queryByText("vless://example")).toBeNull();
    expect(dialog.queryByText("{}")).toBeNull();
  });

  it("sets the shown server active", () => {
    const onSetActive = vi.fn();
    render(
      <ServerDetailsDialog
        profile={profile}
        connected={false}
        onClose={vi.fn()}
        onSetActive={onSetActive}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Set active" }));
    expect(onSetActive).toHaveBeenCalledWith("p-1");
  });

  it("shows a Connected badge only for the connected server", () => {
    const { rerender } = render(
      <ServerDetailsDialog
        profile={profile}
        connected={false}
        onClose={vi.fn()}
        onSetActive={vi.fn()}
      />,
    );
    expect(screen.queryByText("Connected")).toBeNull();

    rerender(
      <ServerDetailsDialog
        profile={profile}
        connected
        onClose={vi.fn()}
        onSetActive={vi.fn()}
      />,
    );
    expect(screen.getByText("Connected")).toBeTruthy();
  });

  it("closes from the close button", () => {
    const onClose = vi.fn();
    render(
      <ServerDetailsDialog
        profile={profile}
        connected={false}
        onClose={onClose}
        onSetActive={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("renders nothing without a profile", () => {
    render(
      <ServerDetailsDialog
        profile={null}
        connected={false}
        onClose={vi.fn()}
        onSetActive={vi.fn()}
      />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
