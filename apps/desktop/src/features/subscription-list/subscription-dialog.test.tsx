import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { FeedServiceListSchema, SubscriptionSchema, type Subscription } from "@mglaroute/contracts";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/invoke", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/invoke")>()),
  invoke: vi.fn(),
}));

import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";
import { SubscriptionDialog } from "./SubscriptionDialog";

const mockedInvoke = vi.mocked(invoke);

const feedUrl = "https://api.mglaroute.ashen-hermit.space/api/feed?format=subscription&search=alpha&protocol=vless&reachable=true&anycast=false&services=svc-1%2Csvc-2&sort=latency_desc&limit=50";

const subscription: Subscription = {
  id: "sub-1",
  name: "Primary feed",
  url: "https://secret.example/sub",
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
};

describe("SubscriptionDialog", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
  });

  beforeEach(() => {
    vi.stubGlobal("scrollTo", vi.fn());
    vi.clearAllMocks();
    useAppStore.setState({ notice: null });
  });

  it("edits name and URL without touching enabled or interval", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_update") {
        return { ...subscription, name: "Renamed", url: "https://new.example/sub" } as never;
      }
      return [] as never;
    });
    render(
      <SubscriptionDialog
        open
        mode="edit"
        subscription={subscription}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />,
    );
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "Renamed" } });
    fireEvent.change(dialog.getByLabelText("Subscription URL"), {
      target: { value: "https://new.example/sub" },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_update", SubscriptionSchema, {
        id: "sub-1",
        name: "Renamed",
        url: "https://new.example/sub",
      }),
    );
    expect(mockedInvoke).not.toHaveBeenCalledWith(
      "subscription_refresh",
      expect.anything(),
      expect.anything(),
    );
  });

  it("prefills feed settings from a mglaroute URL and regenerates the link", async () => {
    mockedInvoke.mockImplementation(async (command, _schema, args) => {
      if (command === "feed_services") {
        return [
          { id: "svc-1", name: "YouTube" },
          { id: "svc-2", name: "Discord" },
        ] as never;
      }
      if (command === "subscription_update") return { ...subscription, ...args } as never;
      return [] as never;
    });
    const saved = vi.fn();
    render(
      <SubscriptionDialog
        open
        mode="edit"
        subscription={{ ...subscription, url: feedUrl }}
        onClose={vi.fn()}
        onSaved={saved}
      />,
    );
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByRole("tab", { name: "Mglaroute" }).getAttribute("data-state")).toBe("active");
    expect((dialog.getByLabelText("Search") as HTMLInputElement).value).toBe("alpha");
    expect((dialog.getByLabelText("Protocol") as HTMLSelectElement).value).toBe("vless");
    expect((dialog.getByLabelText("Reachable") as HTMLSelectElement).value).toBe("true");
    expect((dialog.getByLabelText("Anycast") as HTMLSelectElement).value).toBe("false");
    expect((dialog.getByLabelText("Sort") as HTMLSelectElement).value).toBe("latency_desc");
    expect((dialog.getByLabelText("Limit") as HTMLInputElement).value).toBe("50");
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("feed_services", FeedServiceListSchema),
    );
    expect(
      dialog.getByRole("button", { name: "YouTube" }).getAttribute("data-state"),
    ).toBe("active");
    fireEvent.change(dialog.getByLabelText("Search"), { target: { value: "beta" } });
    fireEvent.click(dialog.getByRole("button", { name: "Generate link" }));
    const generated =
      "https://api.mglaroute.ashen-hermit.space/api/feed?format=subscription&search=beta&protocol=vless&reachable=true&anycast=false&services=svc-1%2Csvc-2&sort=latency_desc&limit=50";
    expect((dialog.getByLabelText("Generated feed URL") as HTMLInputElement).value).toBe(generated);
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(saved).toHaveBeenCalledWith(expect.objectContaining({ url: generated })));
  });

  it("keeps the feed form usable when feed_services fails", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "feed_services") throw { fetch: null };
      return [] as never;
    });
    render(
      <SubscriptionDialog
        open
        mode="add"
        subscription={null}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />,
    );
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.click(dialog.getByRole("tab", { name: "Mglaroute" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("feed_services", FeedServiceListSchema),
    );
    fireEvent.click(dialog.getByRole("button", { name: "Generate link" }));
    expect((dialog.getByLabelText("Generated feed URL") as HTMLInputElement).value).toBe(
      "https://api.mglaroute.ashen-hermit.space/api/feed?format=subscription&sort=latency_asc&limit=200",
    );
  });

  it("validates before saving and omits blank name on add", async () => {
    mockedInvoke.mockImplementation(async () => ({ ...subscription, id: "sub-2", name: "Subscription" }) as never);
    render(
      <SubscriptionDialog
        open
        mode="add"
        subscription={null}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />,
    );
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.click(dialog.getByRole("button", { name: "Add subscription" }));
    expect(mockedInvoke).not.toHaveBeenCalled();
    expect(useAppStore.getState().notice).toContain("Subscription URL is required");

    fireEvent.change(dialog.getByLabelText("Subscription URL"), {
      target: { value: "https://feed.example/sub" },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Add subscription" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_add", SubscriptionSchema, {
        url: "https://feed.example/sub",
      }),
    );
  });

  it("rejects an empty name on edit", async () => {
    render(
      <SubscriptionDialog
        open
        mode="edit"
        subscription={subscription}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />,
    );
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "  " } });
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    expect(mockedInvoke).not.toHaveBeenCalled();
    expect(useAppStore.getState().notice).toContain("Name is required");
  });
});
