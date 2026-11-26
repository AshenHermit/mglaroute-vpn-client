import { z } from "zod";

// Rust emits camelCase payload keys via #[serde(rename_all = "camelCase")].
// Timestamps are ISO-8601 strings from chrono's serde implementation.
const timestamp = z.string();

// Command layer decodes the DB's JSON text and always emits a canonical list.
const testUrls = z.array(z.string());

export const SubscriptionSchema = z.object({
  id: z.string(),
  name: z.string(),
  url: z.string(),
  enabled: z.boolean(),
  status: z.enum(["active", "inactive", "error"]),
  autoRefetchMinutes: z.number().int(),
  lastFetchedAt: z.string().nullable(),
  lastError: z.string().nullable(),
  userAgent: z.string().nullable(),
  requestHeaders: z.string().nullable(),
  sort: z.number().int(),
  createdAt: timestamp,
  updatedAt: timestamp,
});
export type Subscription = z.infer<typeof SubscriptionSchema>;

export const ProfileSchema = z.object({
  id: z.string(),
  subscriptionId: z.string(),
  serverId: z.string(),
  link: z.string(),
  remarks: z.string().nullable(),
  protocol: z.string().nullable(),
  address: z.string().nullable(),
  port: z.number().int().nullable(),
  rawConfig: z.string(),
  present: z.boolean(),
  latencyMs: z.number().int().nullable(),
  reachable: z.boolean(),
  checkedAt: z.string().nullable(),
  sortIndex: z.number().int(),
  createdAt: timestamp,
  updatedAt: timestamp,
});
export type Profile = z.infer<typeof ProfileSchema>;

export const ServiceProfileSchema = z.object({
  id: z.string(),
  name: z.string().min(1),
  testUrls,
  isEnabled: z.boolean(),
  isDefault: z.boolean(),
  createdAt: timestamp,
  updatedAt: timestamp,
});
export type ServiceProfile = z.infer<typeof ServiceProfileSchema>;

const testConcurrency = z.number().int().min(1).max(20);

export const SettingsSchema = z.object({
  autoRefetchEnabled: z.boolean(),
  refetchIntervalMinutes: z.number().int(),
  autoTestEnabled: z.boolean(),
  testIntervalMinutes: z.number().int(),
  testTimeoutMs: z.number().int(),
  // Default keeps older callers valid; settings_get always returns the field.
  testConcurrency: testConcurrency.default(10),
  autoReconnectEnabled: z.boolean(),
  reconnectCooldownSeconds: z.number().int(),
  pingUrl: z.string(),
  localProxyPort: z.number().int(),
  proxyExclusions: z.string(),
  systemProxyEnabled: z.boolean(),
});
export type Settings = z.infer<typeof SettingsSchema>;

// Patch parsing must stay exact: no defaults added to untouched fields.
export const SettingsPatchSchema = SettingsSchema.partial().extend({
  testConcurrency: testConcurrency.optional(),
});
export type SettingsPatch = z.infer<typeof SettingsPatchSchema>;

export const ConnectionStateSchema = z.object({
  status: z.enum(["disconnected", "connecting", "connected", "faulted"]),
  profileId: z.string().nullable(),
  proxyPort: z.number().int().nullable(),
});
export type ConnectionState = z.infer<typeof ConnectionStateSchema>;

export const RefreshResultSchema = z.object({
  added: z.number().int(),
  updated: z.number().int(),
  absent: z.number().int(),
  badLines: z.number().int(),
});
export type RefreshResult = z.infer<typeof RefreshResultSchema>;

export const ProbeServiceResultSchema = z.object({
  serviceProfileId: z.string(),
  reachable: z.boolean(),
});

export const ProbeServerEventSchema = z.object({
  profileId: z.string(),
  status: z.enum(["testing", "done", "failed", "cancelled"]),
  latencyMs: z.number().int().nullable(),
  serviceResults: z.array(ProbeServiceResultSchema),
});
export type ProbeServerEvent = z.infer<typeof ProbeServerEventSchema>;

export const ProbeResultSchema = z.object({
  tested: z.number().int(),
  sorted: z.number().int(),
  finishedAt: timestamp,
  // Older payloads omit it; the native side always sends an explicit flag.
  cancelled: z.boolean().default(false),
});
export type ProbeResult = z.infer<typeof ProbeResultSchema>;

export const AntiDpiStatusSchema = z.object({
  status: z.literal("unsupported"),
});
export type AntiDpiStatus = z.infer<typeof AntiDpiStatusSchema>;

export const UpdateInfoSchema = z.object({
  version: z.string(),
  currentVersion: z.string(),
  notes: z.string().nullable(),
  pubDate: timestamp.nullable(),
  canInstallInApp: z.boolean(),
  releaseUrl: z.string(),
});
export type UpdateInfo = z.infer<typeof UpdateInfoSchema>;

export const UpdateProgressEventSchema = z.object({
  phase: z.enum(["downloading", "installing", "done"]),
  received: z.number().int().nonnegative(),
  total: z.number().int().nonnegative().nullable(),
});
export type UpdateProgressEvent = z.infer<typeof UpdateProgressEventSchema>;

export const EVENTS = {
  probeProgress: "probe-progress",
  probeServer: "probe-server",
  probeCancelled: "probe-cancelled",
  probeFinished: "probe-finished",
  connectionState: "connection-state",
  subscriptionRefreshed: "subscription-refreshed",
  updateProgress: "update-progress",
} as const;
