import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SubscriptionSchema } from "@mglaroute/contracts";

vi.mock("../../lib/invoke", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/invoke")>()),
  invoke: vi.fn(),
}));

import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";
import { SubscriptionList } from "./SubscriptionList";

const mockedInvoke = vi.mocked(invoke);

const errored = {
  id: "sub-1",
  name: "Primary feed",
  url: "https://secret.example/sub",
  enabled: true,
  status: "error" as const,
  autoRefetchMinutes: 60,
  lastFetchedAt: null,
  lastError: "refresh failed",
  userAgent: null,
  requestHeaders: null,
  sort: 0,
  createdAt: "2026-11-24T00:00:00Z",
  updatedAt: "2026-11-24T00:00:00Z",
};

describe("SubscriptionList", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
  });

  beforeEach(() => {
    vi.stubGlobal("scrollTo", vi.fn());
    vi.clearAllMocks();
    useAppStore.setState({ notice: null });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_add") {
        return { ...errored, id: "sub-2", name: "Added feed", status: "active" } as never;
      }
      return [errored] as never;
    });
  });

  it("opens the add dialog and posts URL plus optional name", async () => {
    render(<SubscriptionList />);

    await waitFor(() => expect(screen.getByText("Primary feed")).toBeTruthy());
    const badge = screen.getByTestId("subscription-status-sub-1");
    expect(badge.textContent).toBe("error");
    expect(badge.className).toContain("text-destructive");

    fireEvent.click(screen.getByRole("button", { name: "Add subscription" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Subscription URL"), {
      target: { value: "https://feed.example/sub" },
    });
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "Added feed" } });
    fireEvent.click(dialog.getByRole("button", { name: "Add subscription" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_add", SubscriptionSchema, {
        url: "https://feed.example/sub",
        name: "Added feed",
      }),
    );
  });

  it("renders one card per subscription and opens edit prefilled", async () => {
    mockedInvoke.mockImplementation(async (command, _schema, args) => {
      if (command === "subscription_update") {
        const payload = args as Record<string, unknown>;
        return { ...errored, name: String(payload.name) } as never;
      }
      return [errored] as never;
    });
    render(<SubscriptionList />);
    await waitFor(() => expect(screen.getByText("Primary feed")).toBeTruthy());
    expect(document.querySelectorAll('[data-slot="card"]')).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: /edit primary feed/i }));
    const dialog = within(await screen.findByRole("dialog"));
    expect((dialog.getByLabelText("Name") as HTMLInputElement).value).toBe("Primary feed");
    expect((dialog.getByLabelText("Subscription URL") as HTMLInputElement).value).toBe(
      "https://secret.example/sub",
    );
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "Renamed" } });
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_update", SubscriptionSchema, {
        id: "sub-1",
        name: "Renamed",
        url: "https://secret.example/sub",
      }),
    );
  });

  it("subscription_enable_and_interval_wired", async () => {
    mockedInvoke.mockImplementation(async (command, _schema, args) => {
      if (command === "subscription_update") {
        const payload = args as Record<string, unknown> | undefined;
        return {
          ...errored,
          enabled: typeof payload?.enabled === "boolean" ? payload.enabled : errored.enabled,
          autoRefetchMinutes:
            typeof payload?.autoRefetchMinutes === "number"
              ? payload.autoRefetchMinutes
              : errored.autoRefetchMinutes,
        } as never;
      }
      return [errored] as never;
    });

    const { container } = render(<SubscriptionList />);
    const ui = within(container);
    await waitFor(() => expect(ui.getByText("Primary feed")).toBeTruthy());

    const toggle = ui.getByRole("switch", { name: /enabled for primary feed/i });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(toggle);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_update", SubscriptionSchema, {
        id: "sub-1",
        enabled: false,
      }),
    );

    const interval = ui.getByLabelText(/refetch interval for primary feed/i) as HTMLInputElement;
    expect(interval.value).toBe("60");
    fireEvent.change(interval, { target: { value: "1" } });
    fireEvent.blur(interval);
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_update", SubscriptionSchema, {
        id: "sub-1",
        autoRefetchMinutes: 1,
      }),
    );
  });

  const refreshErrors: Array<[Record<string, null>, string]> = [
    [{ mutation_busy: null }, "Another operation is in progress. Try again shortly."],
    [{ fetch: null }, "Could not reach subscription URL"],
    [{ not_found: null }, "Subscription not found"],
    [{ storage: null }, "Local storage error"],
    [{ probe: null }, "Could not refresh subscription"],
  ];

  it.each(refreshErrors)("maps refresh error %j to notice %s", async (error, expected) => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_refresh") throw error;
      return [errored] as never;
    });

    // Enter the feature directly: the retired nav tab no longer exists. The
    // notice is owned by the shell toast, so assert the store value here.
    render(<SubscriptionList />);
    fireEvent.click(await screen.findByRole("button", { name: "Refresh" }));

    await waitFor(() => expect(useAppStore.getState().notice).toContain(expected));
  });
});
