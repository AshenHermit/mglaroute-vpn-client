import { describe, expect, it } from "vitest";
import {
  AntiDpiStatusSchema,
  ConnectionStateSchema,
  EVENTS,
  FeedServiceListSchema,
  FeedServiceSchema,
  ProfileSchema,
  ProbeResultSchema,
  ProbeServerEventSchema,
  RefreshResultSchema,
  ServiceProfileSchema,
  SettingsPatchSchema,
  SettingsSchema,
  SubscriptionSchema,
  UpdateInfoSchema,
  UpdateProgressEventSchema,
} from "./index";

const timestamp = "2026-01-01T00:00:00Z";

describe("contract schemas", () => {
  it("parses a full subscription using camelCase payload fields", () => {
    const parsed = SubscriptionSchema.parse({
      id: "s",
      name: "sub",
      url: "https://x",
      enabled: true,
      status: "active",
      autoRefetchMinutes: 15,
      lastFetchedAt: null,
      lastError: null,
      userAgent: null,
      requestHeaders: null,
      sort: 0,
      createdAt: timestamp,
      updatedAt: timestamp,
    });
    expect(parsed).toMatchObject({
      id: "s",
      name: "sub",
      enabled: true,
      status: "active",
      autoRefetchMinutes: 15,
      lastFetchedAt: null,
    });
  });

  it("rejects snake_case payload fields and unknown status values", () => {
    const snakeCase = {
      id: "s",
      name: "sub",
      url: "https://x",
      enabled: true,
      status: "active",
      auto_refetch_minutes: 15,
      last_fetched_at: null,
      last_error: null,
      user_agent: null,
      request_headers: null,
      sort: 0,
      created_at: timestamp,
      updated_at: timestamp,
    };
    expect(() => SubscriptionSchema.parse(snakeCase)).toThrow();
    expect(() =>
      SubscriptionSchema.parse({
        ...snakeCase,
        autoRefetchMinutes: 15,
        lastFetchedAt: null,
        lastError: null,
        userAgent: null,
        requestHeaders: null,
        createdAt: timestamp,
        updatedAt: timestamp,
        status: "unknown",
      }),
    ).toThrow();
  });

  it("rejects a service profile with an empty name", () => {
    expect(() =>
      ServiceProfileSchema.parse({
        id: "sp",
        name: "",
        testUrls: ["https://www.youtube.com"],
        isEnabled: true,
        isDefault: false,
        createdAt: timestamp,
        updatedAt: timestamp,
      }),
    ).toThrow();
  });

  it("requires testUrls to be a canonical string list", () => {
    const base = {
      id: "sp",
      name: "YouTube",
      isEnabled: true,
      isDefault: false,
      createdAt: timestamp,
      updatedAt: timestamp,
    };
    expect(ServiceProfileSchema.parse({ ...base, testUrls: ["https://www.youtube.com"] }).testUrls).toEqual([
      "https://www.youtube.com",
    ]);
    expect(() =>
      ServiceProfileSchema.parse({ ...base, testUrls: '["https://www.youtube.com"]' }),
    ).toThrow();
    expect(() => ServiceProfileSchema.parse({ ...base, testUrls: "not-json" })).toThrow();
  });

  it("parses all remaining command payload shapes", () => {
    expect(
      ProfileSchema.parse({
        id: "p",
        subscriptionId: "s",
        serverId: "srv",
        link: "vless://x",
        remarks: null,
        protocol: "vless",
        address: "example.com",
        port: 443,
        rawConfig: "{}",
        present: true,
        latencyMs: 12,
        reachable: true,
        checkedAt: timestamp,
        sortIndex: 10,
        createdAt: timestamp,
        updatedAt: timestamp,
      }).latencyMs,
    ).toBe(12);

    expect(
      SettingsSchema.parse({
        autoRefetchEnabled: true,
        refetchIntervalMinutes: 15,
        autoTestEnabled: true,
        testIntervalMinutes: 15,
        testTimeoutMs: 5000,
        autoReconnectEnabled: false,
        reconnectCooldownSeconds: 60,
        pingUrl: "https://www.google.com/generate_204",
        localProxyPort: 10808,
        proxyExclusions: "[]",
        systemProxyEnabled: true,
      }).localProxyPort,
    ).toBe(10808);

    for (const status of ["disconnected", "connecting", "connected", "faulted"] as const) {
      expect(
        ConnectionStateSchema.parse({ status, profileId: null, proxyPort: null }).status,
      ).toBe(status);
    }
    expect(() =>
      ConnectionStateSchema.parse({ status: "unknown", profileId: null, proxyPort: null }),
    ).toThrow();

    expect(
      RefreshResultSchema.parse({ added: 1, updated: 2, absent: 3, badLines: 4 }),
    ).toEqual({ added: 1, updated: 2, absent: 3, badLines: 4 });

    expect(ProbeResultSchema.parse({ tested: 5, sorted: 4, finishedAt: timestamp })).toEqual({
      tested: 5,
      sorted: 4,
      finishedAt: timestamp,
      cancelled: false,
    });
    expect(
      ProbeServerEventSchema.parse({
        profileId: "p-1",
        status: "done",
        latencyMs: 12,
        serviceResults: [{ serviceProfileId: "svc-1", reachable: true }],
      }),
    ).toEqual({
      profileId: "p-1",
      status: "done",
      latencyMs: 12,
      serviceResults: [{ serviceProfileId: "svc-1", reachable: true }],
    });
    expect(
      ProbeServerEventSchema.parse({
        profileId: "p-2",
        status: "cancelled",
        latencyMs: null,
        serviceResults: [],
      }).status,
    ).toBe("cancelled");

    expect(SettingsPatchSchema.parse({ autoTestEnabled: false })).toEqual({
      autoTestEnabled: false,
    });
    expect(() => SettingsPatchSchema.parse({ testTimeoutMs: "fast" })).toThrow();

    expect(AntiDpiStatusSchema.parse({ status: "unsupported" })).toEqual({
      status: "unsupported",
    });
    expect(() => AntiDpiStatusSchema.parse({ status: "enabled" })).toThrow();
  });

  it("parses update info with camelCase fields", () => {
    expect(UpdateInfoSchema.parse({
      version: "0.2.0",
      currentVersion: "0.1.0",
      notes: "fixes",
      pubDate: "2026-11-26T00:00:00Z",
      canInstallInApp: true,
      releaseUrl: "https://github.com/AshenHermit/mglaroute-vpn-client/releases/tag/v0.2.0",
    })).toMatchObject({ version: "0.2.0", canInstallInApp: true });
  });

  it("rejects snake_case update payloads and unknown progress phases", () => {
    expect(() => UpdateInfoSchema.parse({ version: "0.2.0", current_version: "0.1.0" })).toThrow();
    expect(() => UpdateProgressEventSchema.parse({ phase: "unknown", received: 1, total: null })).toThrow();
  });

  it("maps the update progress event name", () => {
    expect(EVENTS.updateProgress).toBe("update-progress");
  });

  it("parses public feed services and strips admin-only fields", () => {
    expect(
      FeedServiceSchema.parse({
        id: "svc-1",
        name: "YouTube",
        testUrls: ["https://secret.example"],
        isEnabled: true,
        isDefault: true,
      }),
    ).toEqual({ id: "svc-1", name: "YouTube" });
    expect(
      FeedServiceListSchema.parse([{ id: "svc-2", name: "Discord" }]),
    ).toEqual([{ id: "svc-2", name: "Discord" }]);
    expect(() => FeedServiceSchema.parse({ id: "svc-3" })).toThrow();
  });
});
