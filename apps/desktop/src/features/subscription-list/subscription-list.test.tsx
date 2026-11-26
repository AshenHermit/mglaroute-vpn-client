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
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({ notice: null });
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_add") {
        return { ...errored, id: "sub-2", name: "Added feed", status: "active" } as never;
      }
      return [errored] as never;
    });
  });

  it("posts the URL on Add and marks failed subscriptions with a red error badge", async () => {
    render(<SubscriptionList />);

    await waitFor(() => expect(screen.getByText("Primary feed")).toBeTruthy());
    const badge = screen.getByTestId("subscription-status-sub-1");
    expect(badge.textContent).toBe("error");
    expect(badge.className).toContain("text-destructive");

    fireEvent.change(screen.getByLabelText(/subscription url/i), {
      target: { value: "https://feed.example/sub" },
    });
    fireEvent.click(screen.getByRole("button", { name: /add subscription/i }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "subscription_add",
        SubscriptionSchema,
        { url: "https://feed.example/sub" },
      ),
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
      if (command === "subscription_add") {
        return { ...errored, id: "sub-2", name: "Added feed", status: "active" } as never;
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

    fireEvent.change(ui.getByPlaceholderText(/example\.com\/subscription/i), {
      target: { value: "https://feed.example/sub" },
    });
    fireEvent.click(ui.getByRole("button", { name: /add subscription/i }));
    await waitFor(() => expect(ui.getByText("Added feed")).toBeTruthy());
    const addedInterval = ui.getByLabelText(/refetch interval for added feed/i) as HTMLInputElement;
    expect(Number(addedInterval.value)).toBeGreaterThan(0);
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

  it("uses container queries so the Add button stays visible in a narrow panel", async () => {
    render(<SubscriptionList />);
    await waitFor(() => expect(screen.getByText("Primary feed")).toBeTruthy());

    const grid = screen.getByLabelText("Subscription URL").closest("div")?.parentElement;
    expect(grid?.className).toContain("@lg:grid-cols-[minmax(0,1fr)_10rem_auto]");
    expect(grid?.className).not.toContain("sm:grid-cols");
    expect(grid?.parentElement?.className).toContain("@container");
    expect(screen.getByRole("button", { name: "Add subscription" }).className).toContain(
      "shrink-0",
    );

    const intervalLabel = screen.getByText(/Refetch interval for Primary feed/);
    expect(intervalLabel.className).toContain("break-words");
  });
});
