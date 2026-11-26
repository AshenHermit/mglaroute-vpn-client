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

import { invoke } from "../lib/invoke";
import { useAppStore } from "../stores/app.store";
import { AppShell } from "../app-shell/AppShell";

const mockedInvoke = vi.mocked(invoke);

const settingsTabsVisible = () =>
  ["Subscriptions", "Service profiles", "Settings", "Updates"].every((name) =>
    Boolean(screen.queryByRole("tab", { name })),
  );

describe("Settings page navigation", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (command) => {
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
});
