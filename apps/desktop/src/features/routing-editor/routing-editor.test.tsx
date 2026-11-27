import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../lib/invoke";
import { RoutingEditor } from "./RoutingEditor";

const mockedInvoke = vi.mocked(invoke);
const timestamp = "2026-11-27T00:00:00Z";

const profiles = [
  {
    id: "rp-1",
    name: "Bypass local",
    enabled: true,
    isActive: true,
    sort: 0,
    createdAt: timestamp,
    updatedAt: timestamp,
  },
  {
    id: "rp-2",
    name: "Proxy all",
    enabled: true,
    isActive: false,
    sort: 1,
    createdAt: timestamp,
    updatedAt: timestamp,
  },
];

const rules = [
  {
    id: "rr-1",
    profileId: "rp-1",
    remark: "Private networks",
    enabled: true,
    outbound: "direct",
    domains: ["geosite:private"],
    ips: ["geoip:private", "10.0.0.0/8"],
    sort: 0,
    createdAt: timestamp,
    updatedAt: timestamp,
  },
  {
    id: "rr-2",
    profileId: "rp-1",
    remark: "Block ads",
    enabled: true,
    outbound: "block",
    domains: ["geosite:category-ads-all"],
    ips: [],
    sort: 1,
    createdAt: timestamp,
    updatedAt: timestamp,
  },
];

describe("RoutingEditor", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (command, _schema, args) => {
      if (command === "routing_profile_list") return profiles as never;
      if (command === "routing_rule_list") {
        const profileId = (args as { profileId?: string } | undefined)?.profileId;
        return (profileId === "rp-1" ? rules : []) as never;
      }
      if (command === "routing_profile_create") {
        return { ...profiles[1], id: "rp-new", name: "New profile" } as never;
      }
      if (command === "routing_profile_activate") {
        return { ...profiles[1], isActive: true } as never;
      }
      if (command === "routing_rule_create") {
        return {
          id: "rr-new",
          profileId: "rp-1",
          remark: "New rule",
          enabled: true,
          outbound: "block",
          domains: [],
          ips: [],
          sort: 2,
          createdAt: timestamp,
          updatedAt: timestamp,
        } as never;
      }
      if (command === "routing_rule_update") {
        const patch = args as Record<string, unknown>;
        return { ...rules[0], ...patch } as never;
      }
      if (command === "routing_rule_reorder") return null as never;
      return null as never;
    });
  });

  it("loads profiles and rules and switches selection", async () => {
    render(<RoutingEditor />);

    expect(await screen.findByText("Bypass local")).toBeTruthy();
    expect(screen.getByText("Proxy all")).toBeTruthy();
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_rule_list",
        expect.anything(),
        { profileId: "rp-1" },
      ),
    );
    expect(await screen.findByText("Private networks")).toBeTruthy();
    expect(screen.getByText("Block ads")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Proxy all" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_rule_list",
        expect.anything(),
        { profileId: "rp-2" },
      ),
    );
    expect(screen.queryByText("Private networks")).toBeNull();
  });

  it("creates a rule with split textarea entries and outbound block", async () => {
    render(<RoutingEditor />);
    expect(await screen.findByText("Private networks")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "+ Rule" }));

    fireEvent.change(screen.getByLabelText("Remark"), {
      target: { value: "Custom" },
    });
    fireEvent.change(screen.getByLabelText("Domains"), {
      target: { value: "reg.ru\ngeosite:private" },
    });
    fireEvent.change(screen.getByLabelText("IP / CIDR"), {
      target: { value: "geoip:private, 10.0.0.0/8" },
    });
    fireEvent.change(screen.getByLabelText("Outbound"), {
      target: { value: "block" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save rule" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_rule_create",
        expect.anything(),
        {
          profileId: "rp-1",
          remark: "Custom",
          enabled: true,
          outbound: "block",
          domains: ["reg.ru", "geosite:private"],
          ips: ["geoip:private", "10.0.0.0/8"],
        },
      ),
    );
  });

  it("toggles and reorders a rule", async () => {
    render(<RoutingEditor />);
    expect(await screen.findByText("Private networks")).toBeTruthy();

    fireEvent.click(screen.getByRole("checkbox", { name: "Enable Private networks" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_rule_update",
        expect.anything(),
        {
          id: "rr-1",
          remark: "Private networks",
          enabled: false,
          outbound: "direct",
          domains: ["geosite:private"],
          ips: ["geoip:private", "10.0.0.0/8"],
        },
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "Private networks" }));
    fireEvent.click(screen.getByRole("button", { name: "Move down" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_rule_reorder",
        expect.anything(),
        { id: "rr-1", direction: 1 },
      ),
    );
  });

  it("activates a profile", async () => {
    render(<RoutingEditor />);
    expect(await screen.findByText("Bypass local")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Activate Proxy all" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_profile_activate",
        expect.anything(),
        { id: "rp-2" },
      ),
    );
  });

  it("creates a profile from the name field", async () => {
    render(<RoutingEditor />);
    expect(await screen.findByText("Bypass local")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("New profile name"), {
      target: { value: "New profile" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add routing profile" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith(
        "routing_profile_create",
        expect.anything(),
        { name: "New profile" },
      ),
    );
  });
});
