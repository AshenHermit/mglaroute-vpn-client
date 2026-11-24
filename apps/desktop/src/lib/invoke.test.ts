import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { RefreshResultSchema } from "@mglaroute/contracts";
import { invoke } from "./invoke";

const mockedInvoke = vi.mocked(tauriInvoke);

describe("typed invoke", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("returns the parsed Tauri result", async () => {
    mockedInvoke.mockResolvedValue({ added: 1, updated: 2, absent: 3, badLines: 4 });
    await expect(
      invoke("subscription_refresh", RefreshResultSchema, { id: "sub-1" }),
    ).resolves.toEqual({ added: 1, updated: 2, absent: 3, badLines: 4 });
    expect(mockedInvoke).toHaveBeenCalledWith("subscription_refresh", { id: "sub-1" });
  });

  it("throws a command-named error on schema mismatch without echoing args", async () => {
    mockedInvoke.mockResolvedValue({
      added: "not-a-number",
      updated: 2,
      absent: 3,
      badLines: 4,
    });
    let caught: unknown;
    try {
      await invoke("subscription_refresh", RefreshResultSchema, {
        id: "sub-1",
        url: "https://secret.example/token",
      });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(Error);
    const message = (caught as Error).message;
    expect(message).toContain("subscription_refresh");
    expect(message).not.toContain("secret.example");
    expect(message).not.toContain("sub-1");
  });

  it("does not leak native error text", async () => {
    mockedInvoke.mockRejectedValue(new Error("GET https://secret.example/token failed"));
    let caught: unknown;
    try {
      await invoke("subscription_refresh", RefreshResultSchema, { id: "sub-1" });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(Error);
    const message = (caught as Error).message;
    expect(message).toContain("subscription_refresh");
    expect(message).not.toContain("secret.example");
  });
});
