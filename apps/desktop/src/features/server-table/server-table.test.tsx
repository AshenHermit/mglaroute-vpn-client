import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Profile } from "@mglaroute/contracts";
import { ServerTable } from "./ServerTable";

function profile(
  id: string,
  latencyMs: number | null,
  reachable: boolean,
  sortIndex: number,
): Profile {
  return {
    id,
    subscriptionId: "sub-1",
    serverId: id,
    link: "vless://secret@node.example.com:443",
    remarks: id,
    protocol: "vless",
    address: "node.example.com",
    port: 443,
    rawConfig: "{}",
    present: true,
    latencyMs,
    reachable,
    checkedAt: null,
    sortIndex,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  };
}

describe("ServerTable", () => {
  it("sorts by ascending delay and pushes failed servers last, greyed out", () => {
    render(
      <ServerTable
        profiles={[
          profile("p-slow", 40, true, 0),
          profile("p-failed", 5, false, 1),
          profile("p-fast", 10, true, 2),
          profile("p-mid", 25, true, 3),
        ]}
      />,
    );

    const rows = screen.getAllByTestId("server-row");
    expect(rows.map((row) => row.getAttribute("data-profile-id"))).toEqual([
      "p-fast",
      "p-mid",
      "p-slow",
      "p-failed",
    ]);
    expect(rows[0].textContent).toContain("10 ms");
    expect(rows[1].textContent).toContain("25 ms");
    expect(rows[3].textContent).toContain("failed");
    expect(rows[3].className).toContain("opacity-60");
  });

  it("keeps long server names inside a scrollable table area", () => {
    const { container } = render(
      <ServerTable profiles={[profile("p-1", 10, true, 0)]} />,
    );

    expect(container.querySelector("div.overflow-x-auto")).toBeTruthy();
    expect(container.querySelector("table")?.className).toContain("table-fixed");
    expect(container.querySelectorAll("td.break-words").length).toBeGreaterThan(0);
  });

  it("reports a clicked row and reflects the selected id", () => {
    const onSelect = vi.fn();
    const rows = [profile("p-1", 10, true, 0), profile("p-2", 20, true, 1)];
    const { rerender } = render(
      <ServerTable profiles={rows} selectedId={null} onSelect={onSelect} />,
    );

    const row = screen
      .getAllByTestId("server-row")
      .find((candidate) => candidate.getAttribute("data-profile-id") === "p-2");
    expect(row).toBeTruthy();
    expect(row?.getAttribute("role")).toBe("button");
    expect(row?.getAttribute("data-selected")).toBe("false");

    fireEvent.click(row!);
    expect(onSelect).toHaveBeenCalledWith("p-2");

    rerender(
      <ServerTable profiles={rows} selectedId="p-2" onSelect={onSelect} />,
    );
    const selected = screen
      .getAllByTestId("server-row")
      .find((candidate) => candidate.getAttribute("data-profile-id") === "p-2");
    expect(selected?.getAttribute("data-selected")).toBe("true");
  });
});
