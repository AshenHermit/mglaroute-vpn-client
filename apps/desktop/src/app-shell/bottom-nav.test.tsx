import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BottomNav } from "./BottomNav";

type Tab = "connect" | "servers" | "more";

describe("BottomNav", () => {
  afterEach(cleanup);

  function setup(active: Tab = "connect") {
    const onConnect = vi.fn();
    const onServers = vi.fn();
    const onMore = vi.fn();
    render(
      <BottomNav
        active={active}
        onConnect={onConnect}
        onServers={onServers}
        onMore={onMore}
      />,
    );
    return { onConnect, onServers, onMore };
  }

  it("renders three labelled, touch-sized tabs", () => {
    setup();

    for (const name of ["Connect", "Servers", "More"] as const) {
      expect(screen.getByRole("button", { name }).className).toContain("min-h-11");
    }
  });

  it("wires each tab to its own callback, with More as the only settings opener", () => {
    const { onConnect, onServers, onMore } = setup();

    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onServers).not.toHaveBeenCalled();
    expect(onMore).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Servers" }));
    expect(onServers).toHaveBeenCalledTimes(1);
    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onMore).not.toHaveBeenCalled();

    // Callback wiring is the only opener: Connect and Servers never touch it.
    fireEvent.click(screen.getByRole("button", { name: "More" }));
    expect(onMore).toHaveBeenCalledTimes(1);
    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onServers).toHaveBeenCalledTimes(1);
  });

  it("marks only the current tab as active", () => {
    setup("servers");

    expect(screen.getByRole("button", { name: "Servers" }).getAttribute("aria-current")).toBe(
      "page",
    );
    expect(screen.getByRole("button", { name: "Connect" }).getAttribute("aria-current")).toBeNull();
    expect(screen.getByRole("button", { name: "More" }).getAttribute("aria-current")).toBeNull();
  });
});
