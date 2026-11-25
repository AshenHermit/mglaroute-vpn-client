import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

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
  autoReconnectEnabled: false,
  reconnectCooldownSeconds: 30,
  pingUrl: "https://www.google.com/generate_204",
  localProxyPort: 10808,
  proxyExclusions: "localhost",
  systemProxyEnabled: true,
};

describe("SettingsForm", () => {
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
});
