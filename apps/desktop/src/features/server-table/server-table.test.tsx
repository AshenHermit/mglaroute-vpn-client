import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
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
  afterEach(cleanup);

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

  it("keeps long server names single-line with a title inside a scrollable table area", () => {
    const name = "us United States | TG:@HardVpn_robot_bot_t.me/jsxrd";
    const item = { ...profile("p-1", 10, true, 0), remarks: name };
    const { container } = render(<ServerTable profiles={[item]} />);

    expect(container.querySelector("div.overflow-x-auto")).toBeTruthy();
    expect(container.querySelector("table")?.className).toContain("table-fixed");

    const cell = screen.getByText(name).closest("td")!;
    expect(cell.className).toContain("truncate");
    expect(cell.getAttribute("title")).toBe(name);

    const headerRow = screen.getByText("Server").closest("tr")!;
    for (const heading of headerRow.querySelectorAll("th")) {
      expect(heading.className).toContain("whitespace-nowrap");
    }
  });

  it("row body click does not select a server", () => {
    const onSelect = vi.fn();
    render(<ServerTable profiles={[profile("p-1", 10, true, 0)]} onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId("server-row"));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("power button sets the active server", () => {
    const onSelect = vi.fn();
    render(
      <ServerTable
        profiles={[profile("p-1", 10, true, 0), profile("p-2", 20, true, 1)]}
        selectedId="p-1"
        onSelect={onSelect}
      />,
    );
    const buttons = screen.getAllByTestId("server-power");
    const p2 = buttons.find((b) => b.getAttribute("data-profile-id") === "p-2");
    fireEvent.click(p2!);
    expect(onSelect).toHaveBeenCalledWith("p-2");
  });

  it("renders three power states", () => {
    const { rerender } = render(
      <ServerTable profiles={[profile("p-1", 10, true, 0)]} selectedId={null} />,
    );
    expect(screen.getByTestId("server-power").getAttribute("data-state")).toBe("inactive");

    rerender(<ServerTable profiles={[profile("p-1", 10, true, 0)]} selectedId="p-1" />);
    expect(screen.getByTestId("server-power").getAttribute("data-state")).toBe("active");

    rerender(
      <ServerTable profiles={[profile("p-1", 10, true, 0)]} selectedId="p-1" connectedId="p-1" />,
    );
    expect(screen.getByTestId("server-power").getAttribute("data-state")).toBe("connected");
  });

  it("toggles a favorite from the star without selecting the server", () => {
    const onToggleFavorite = vi.fn();
    const onSelect = vi.fn();
    const { rerender } = render(
      <ServerTable
        profiles={[profile("p-1", 10, true, 0)]}
        favorites={[]}
        onToggleFavorite={onToggleFavorite}
        onSelect={onSelect}
      />,
    );

    const star = screen.getByTestId("server-favorite");
    expect(star.getAttribute("aria-label")).toBe("Add favorite: p-1");
    fireEvent.click(star);
    expect(onToggleFavorite).toHaveBeenCalledWith("p-1");
    expect(onSelect).not.toHaveBeenCalled();

    rerender(
      <ServerTable
        profiles={[profile("p-1", 10, true, 0)]}
        favorites={["p-1"]}
        onToggleFavorite={onToggleFavorite}
        onSelect={onSelect}
      />,
    );
    expect(screen.getByTestId("server-favorite").getAttribute("aria-label")).toBe(
      "Remove favorite: p-1",
    );
  });

  it("opens details with the clicked profile", () => {
    const onOpenDetails = vi.fn();
    const item = profile("p-1", 10, true, 0);
    render(<ServerTable profiles={[item]} onOpenDetails={onOpenDetails} />);

    const details = screen.getByTestId("server-details");
    expect(details.getAttribute("aria-label")).toBe("Server details: p-1");
    fireEvent.click(details);
    expect(onOpenDetails).toHaveBeenCalledWith(item);
  });

  it("hides Protocol and Address columns below sm but keeps a single table", () => {
    const { container } = render(<ServerTable profiles={[profile("p-1", 10, true, 0)]} />);

    expect(container.querySelectorAll("table")).toHaveLength(1);
    const hiddenBelowSm = (className: string) => {
      const tokens = className.split(" ");
      return tokens.includes("hidden") && tokens.includes("sm:table-cell");
    };
    for (const heading of ["Protocol", "Address"]) {
      expect(hiddenBelowSm(screen.getByText(heading).className)).toBe(true);
    }
    expect(hiddenBelowSm(screen.getByText("vless").className)).toBe(true);
    expect(hiddenBelowSm(screen.getByText("node.example.com").className)).toBe(true);
  });
});
