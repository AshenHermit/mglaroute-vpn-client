import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("./Servers", () => ({
  default: () => <div data-testid="servers-marker" />,
}));
vi.mock("../features/connection-panel/ConnectionPanel", () => ({
  ConnectionPanel: () => <div data-testid="connection-marker" />,
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
vi.mock("../features/routing-editor/RoutingEditor", () => ({
  RoutingEditor: () => <div data-testid="routing-marker" />,
}));

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { AppShell } from "../app-shell/AppShell";

const mockedInvoke = vi.mocked(invoke);

const settingsTabsVisible = () =>
  ["Subscriptions", "Service profiles", "Routing", "Settings", "Updates"].every((name) =>
    Boolean(screen.queryByRole("tab", { name })),
  );

const desktopTabsVisible = () =>
  ["Subscriptions", "Service profiles", "Routing", "Settings"].every((name) =>
    Boolean(screen.queryByRole("tab", { name })),
  );

const openSettings = () => {
  render(<AppShell />);
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
};

describe("Settings page navigation", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "platform_info") return { mobile: false } as never;
      if (command === "subscription_list") return [] as never;
      if (command === "profile_list") return [] as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      return undefined as never;
    });
    act(() => {
      useAppStore.setState({ subscriptions: [], profiles: [] });
    });
  });

  it("replaces the servers page with the settings page from the rail gear", () => {
    render(<AppShell />);
    expect(screen.getByTestId("servers-marker")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));

    expect(screen.queryByTestId("servers-marker")).toBeNull();
    expect(settingsTabsVisible()).toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("rail-zone")).toBeTruthy();
    expect(screen.getByTestId("bottom-nav")).toBeTruthy();
  });

  it("shows a five-column Routing tab and renders its editor", () => {
    render(<AppShell />);
    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));

    expect(screen.getByRole("tablist").className).toContain("grid-cols-5");
    expect(screen.getByRole("tablist").className).not.toContain("grid-cols-4");

    fireEvent.click(screen.getByRole("tab", { name: "Routing" }));
    expect(screen.getByTestId("routing-marker")).toBeTruthy();
  });

  it("opens the same settings page from the bottom-nav More tab", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByTestId("bottom-nav-more"));

    expect(screen.queryByTestId("servers-marker")).toBeNull();
    expect(settingsTabsVisible()).toBe(true);
    expect(screen.getByTestId("bottom-nav-more").getAttribute("aria-current")).toBe("page");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("returns to the server table from the settings page", () => {
    render(<AppShell />);

    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
    expect(settingsTabsVisible()).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "All servers" }));

    expect(screen.getByTestId("servers-marker")).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Subscriptions" })).toBeNull();
  });

  it("keeps the Updates tab and UpdatePanel on desktop", async () => {
    openSettings();

    expect(await screen.findByRole("tab", { name: "Updates" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Updates" }));
    expect(screen.getByTestId("update-panel")).toBeTruthy();
    expect(
      mockedInvoke.mock.calls.filter(([command]) => command === "platform_info"),
    ).toHaveLength(1);
    expect(
      mockedInvoke.mock.calls.filter(([command]) => command === "anti_dpi_status"),
    ).toHaveLength(1);
  });

  it("hides the Updates tab and UpdatePanel on mobile", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "platform_info") return { mobile: true } as never;
      if (command === "subscription_list") return [] as never;
      if (command === "profile_list") return [] as never;
      if (command === "anti_dpi_status") return { status: "unsupported" } as never;
      return undefined as never;
    });

    openSettings();

    expect(desktopTabsVisible()).toBe(true);
    expect(await screen.findByTestId("anti-dpi-status")).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Updates" })).toBeNull();
    expect(screen.queryByTestId("update-panel")).toBeNull();
    expect(screen.queryByRole("button", { name: /anti-dpi/i })).toBeNull();
    expect(
      mockedInvoke.mock.calls.filter(([command]) => command === "platform_info"),
    ).toHaveLength(1);
  });
});
