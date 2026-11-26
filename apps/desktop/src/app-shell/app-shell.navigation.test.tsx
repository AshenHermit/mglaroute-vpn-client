import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Subscription } from "@mglaroute/contracts";

vi.mock("../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("../features/subscription-list/SubscriptionList", () => ({
  SubscriptionList: () => <div data-testid="subscriptions-marker" />,
}));
vi.mock("../features/service-profile-editor/ServiceProfileEditor", () => ({
  ServiceProfileEditor: () => <div data-testid="service-profiles-marker" />,
}));
vi.mock("../features/settings-form/SettingsForm", () => ({
  SettingsForm: () => <div data-testid="settings-marker" />,
}));

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { disconnected, useConnectionStore } from "../stores/connection.store";
import { useUiStore } from "../stores/ui.store";
import { AppShell } from "./AppShell";

const mockedInvoke = vi.mocked(invoke);

// jsdom has no scrollIntoView. Capture the receiver so assertions can name the
// zone actually scrolled, not just the fact that a stub was called.
const scrolledZones: string[] = [];
const scrollIntoView = vi.fn(function scrollIntoView(this: Element) {
  scrolledZones.push(this.getAttribute("data-testid") ?? "unknown");
});
Element.prototype.scrollIntoView = scrollIntoView;

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
];

describe("AppShell portrait navigation", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    scrolledZones.length = 0;
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
      if (command === "profile_list") return [] as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      if (command === "connection_status") return disconnected as never;
      return undefined as never;
    });
  });

  it("scrolls the center zone when the Servers tab is tapped", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByTestId("bottom-nav-servers"));

    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrolledZones).toEqual(["center-zone"]);
  });

  it("scrolls the right zone when the Connect tab is tapped", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByTestId("bottom-nav-connect"));

    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrolledZones).toEqual(["right-zone"]);
  });

  it("follows the last tap without a scroll listener", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByTestId("bottom-nav-servers"));
    fireEvent.scroll(screen.getByTestId("right-zone"));
    expect(screen.getByTestId("bottom-nav-servers").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("bottom-nav-connect").getAttribute("aria-current")).toBeNull();

    fireEvent.click(screen.getByTestId("bottom-nav-connect"));
    expect(screen.getByTestId("bottom-nav-connect").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("bottom-nav-servers").getAttribute("aria-current")).toBeNull();
    expect(scrolledZones).toEqual(["center-zone", "right-zone"]);
  });

  it("opens the settings page from More and keeps More active", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByTestId("bottom-nav-more"));
    expect(screen.getByRole("tab", { name: "Subscriptions" })).toBeTruthy();
    expect(screen.queryByTestId("subscriptions-marker")).toBeTruthy();
    expect(screen.getByTestId("bottom-nav-more").getAttribute("aria-current")).toBe("page");
    expect(scrolledZones).toEqual([]);
  });

  it("marks More active when the rail gear opens settings", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
    expect(screen.getByRole("tab", { name: "Subscriptions" })).toBeTruthy();
    expect(screen.getByTestId("bottom-nav-more").getAttribute("aria-current")).toBe("page");

    fireEvent.click(screen.getByTestId("bottom-nav-servers"));
    expect(screen.getByTestId("bottom-nav-servers").getAttribute("aria-current")).toBe("page");
    expect(screen.queryByRole("tab", { name: "Subscriptions" })).toBeNull();
  });

  it("uses the shell root as the portrait scroll container and a grid from sm up", () => {
    render(<AppShell />);

    const root = screen.getByTestId("center-zone").parentElement;
    expect(root?.className).toContain("flex-col");
    expect(root?.className).toContain("overflow-y-auto");
    expect(root?.className).toContain("sm:grid");
    expect(root?.className).toContain("sm:overflow-hidden");

    for (const id of ["rail-zone", "center-zone", "right-zone"]) {
      const zone = screen.getByTestId(id);
      const classes = zone.className.split(" ");
      expect(classes).toContain("sm:min-h-0");
      expect(classes).toContain("sm:overflow-y-auto");
      expect(classes).toContain("sm:overscroll-contain");
      expect(classes).not.toContain("min-h-0");
      expect(classes).not.toContain("overflow-y-auto");
    }
    expect(screen.getByTestId("right-zone").className).toContain(
      "pb-[calc(72px+env(safe-area-inset-bottom))]",
    );
  });

  it("labels every rail filter, including subscriptions, for the icon-only rail", async () => {
    render(<AppShell />);

    for (const name of ["All servers", "Favorites", "Reachable only", "Feed one"]) {
      const button = await screen.findByRole("button", { name });
      expect(button.getAttribute("title")).toBe(name);
      const label = button.querySelector("span");
      expect(label?.textContent).toBe(name);
      expect(label?.className).toContain("sm:hidden");
      expect(label?.className).toContain("lg:inline");
      expect(button.querySelector("svg")).toBeTruthy();
    }
  });
});
