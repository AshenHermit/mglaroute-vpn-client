import { describe, expect, it } from "vitest";
import { formatTimestamp } from "./datetime";

describe("formatTimestamp", () => {
  it("formats an ISO timestamp in Moscow local time", () => {
    expect(formatTimestamp("2026-11-26T10:00:00Z")).toBe("26.11.2026 13:00:00");
  });

  it("ignores nanoseconds when formatting", () => {
    expect(formatTimestamp("2026-11-26T10:54:58.641310500Z")).toBe("26.11.2026 13:54:58");
  });
});
