import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../lib/invoke";
import { SettingsForm } from "./SettingsForm";

const mockedInvoke = vi.mocked(invoke);

const settings = {
  autoRefetchEnabled: true,
  refetchIntervalMinutes: 60,
  autoTestEnabled: true,
  testIntervalMinutes: 10,
  testTimeoutMs: 2000,
  testConcurrency: 10,
  autoReconnectEnabled: false,
  reconnectCooldownSeconds: 30,
  pingUrl: "https://www.google.com/generate_204",
  localProxyPort: 10808,
  proxyExclusions: "localhost",
  systemProxyEnabled: true,
};

describe("SettingsForm", () => {
  afterEach(() => cleanup());

  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async () => settings as never);
  });

  it("persists an auto-refetch toggle through settings_update", async () => {
    render(<SettingsForm />);

    const toggle = await screen.findByRole("switch", { name: /auto refetch/i });
    expect(toggle.getAttribute("aria-checked")).toBe("true");

    fireEvent.click(toggle);

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "settings_update",
        expect.anything(),
        { autoRefetchEnabled: false },
      ),
    );
  });

  it("edits and saves test concurrency through settings_update", async () => {
    render(<SettingsForm />);

    const input = await screen.findByRole("spinbutton", { name: /test concurrency/i });
    expect((input as HTMLInputElement).value).toBe("10");

    fireEvent.change(input, { target: { value: "12" } });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "settings_update",
        expect.anything(),
        expect.objectContaining({ testConcurrency: 12 }),
      ),
    );
  });

  it("constrains test concurrency to 1..20 inline", async () => {
    render(<SettingsForm />);
    const input = (await screen.findByRole("spinbutton", {
      name: /test concurrency/i,
    })) as HTMLInputElement;
    expect(input.min).toBe("1");
    expect(input.max).toBe("20");
  });

  it("offers retry instead of hanging on settings load failure", async () => {
    mockedInvoke.mockRejectedValueOnce(new Error("boom"));
    render(<SettingsForm />);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Could not load settings. Try again once other operations finish.",
    );
    expect(screen.queryByText("Loading settings…")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(
      await screen.findByRole("switch", { name: /auto refetch/i }),
    ).toBeTruthy();
  });
});
