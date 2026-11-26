import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUiStore } from "../stores/ui.store";

vi.mock("../features/subscription-list/SubscriptionList", () => ({
  SubscriptionList: () => <div data-testid="subscriptions-marker" />,
}));
vi.mock("../features/service-profile-editor/ServiceProfileEditor", () => ({
  ServiceProfileEditor: () => <div data-testid="service-profiles-marker" />,
}));
vi.mock("../features/settings-form/SettingsForm", () => ({
  SettingsForm: () => <div data-testid="settings-marker" />,
}));

import { GearSheet } from "./GearSheet";

// animate-ui keeps all panels mounted; inactive ones are inert/height-clipped.
const markerPanel = (marker: string): HTMLElement => {
  const panel = screen.getByTestId(marker).closest('[role="tabpanel"]');
  if (!panel) throw new Error(`${marker} is not inside a tabpanel`);
  return panel as HTMLElement;
};

describe("GearSheet", () => {
  afterEach(cleanup);

  beforeEach(() => {
    useUiStore.setState({ sheetTab: "subscriptions" });
  });

  it("hosts the three real tabs, switches store state, and closes via the dialog control", () => {
    const onOpenChange = vi.fn();
    render(<GearSheet open onOpenChange={onOpenChange} />);

    expect(screen.getByRole("tab", { name: "Subscriptions" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Service profiles" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Settings" })).toBeTruthy();

    const dialog = screen.getByRole("dialog");
    expect(dialog.className).toContain("sm:w-[min(480px,100vw)]");
    expect(within(markerPanel("subscriptions-marker")).getByTestId("subscriptions-marker")).toBeTruthy();
    expect(markerPanel("subscriptions-marker").getAttribute("data-state")).toBe("active");
    expect(markerPanel("settings-marker").hasAttribute("inert")).toBe(true);

    fireEvent.click(screen.getByRole("tab", { name: "Settings" }));

    expect(useUiStore.getState().sheetTab).toBe("settings");
    const settingsPanel = markerPanel("settings-marker");
    expect(settingsPanel.getAttribute("data-state")).toBe("active");
    expect(within(settingsPanel).getByTestId("settings-marker")).toBeTruthy();
    expect(markerPanel("subscriptions-marker").hasAttribute("inert")).toBe(true);
    expect(markerPanel("service-profiles-marker").hasAttribute("inert")).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Close settings" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
