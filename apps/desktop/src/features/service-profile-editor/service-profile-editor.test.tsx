import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../lib/invoke";
import { ServiceProfileEditor } from "./ServiceProfileEditor";

const mockedInvoke = vi.mocked(invoke);

describe("ServiceProfileEditor", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "service_profile_create") {
        return {
          id: "svc-1",
          name: "Critical services",
          testUrls: ["https://svc.example/health"],
          isEnabled: true,
          isDefault: false,
          createdAt: "2026-11-24T00:00:00Z",
          updatedAt: "2026-11-24T00:00:00Z",
        } as never;
      }
      return [] as never;
    });
  });

  it("rejects an empty URL list without invoking create, then creates a valid profile", async () => {
    render(<ServiceProfileEditor />);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "service_profile_list",
        expect.anything(),
      ),
    );

    fireEvent.change(screen.getByLabelText(/profile name/i), {
      target: { value: "Critical services" },
    });
    fireEvent.click(screen.getByRole("button", { name: /add profile/i }));

    expect(screen.getByRole("alert").textContent).toMatch(
      /at least one test url/i,
    );
    expect(
      mockedInvoke.mock.calls.some(([command]) => command === "service_profile_create"),
    ).toBe(false);

    fireEvent.change(screen.getByLabelText(/test urls/i), {
      target: { value: "https://svc.example/health" },
    });
    fireEvent.click(screen.getByRole("button", { name: /add profile/i }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "service_profile_create",
        expect.anything(),
        {
          name: "Critical services",
          testUrls: ["https://svc.example/health"],
        },
      ),
    );
  });
});
