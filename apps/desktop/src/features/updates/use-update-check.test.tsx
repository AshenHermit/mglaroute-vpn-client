import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { UpdateInfo } from "@mglaroute/contracts";
import { invoke } from "../../lib/invoke";

vi.mock("../../lib/invoke", () => ({ invoke: vi.fn() }));

const mocks = vi.hoisted(() => ({
  setUpdate: vi.fn(),
  setNotice: vi.fn(),
}));

vi.mock("../../stores/app.store", () => ({
  useAppStore: (selector: (store: unknown) => unknown) => selector(mocks),
}));

import { UPDATE_CHECK_INTERVAL_MS, useUpdateCheck } from "./use-update-check";

const mockedInvoke = vi.mocked(invoke);

const info: UpdateInfo = {
  version: "0.2.0",
  currentVersion: "0.1.0",
  notes: null,
  pubDate: null,
  canInstallInApp: true,
  releaseUrl: "https://example.com/releases/v0.2.0",
};

describe("useUpdateCheck", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockedInvoke.mockReset();
    mocks.setUpdate.mockReset();
    mocks.setNotice.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("checks once on mount, stores the update and announces a new version", async () => {
    mockedInvoke.mockResolvedValue(info);

    renderHook(() => useUpdateCheck());

    expect(mockedInvoke).toHaveBeenCalledTimes(1);
    const [command, schema] = mockedInvoke.mock.calls[0]!;
    expect(command).toBe("update_check");
    expect(schema.safeParse(info).success).toBe(true);
    expect(schema.safeParse(null).success).toBe(true);

    await act(async () => {
      await Promise.resolve();
    });

    expect(mocks.setUpdate).toHaveBeenCalledWith(info);
    expect(mocks.setNotice).toHaveBeenCalledTimes(1);
    expect(mocks.setNotice).toHaveBeenCalledWith("Update available: 0.2.0");
  });

  it("announces a version at most once across interval ticks", async () => {
    mockedInvoke.mockResolvedValue(info);

    renderHook(() => useUpdateCheck());
    await act(async () => {
      await Promise.resolve();
    });
    expect(mocks.setNotice).toHaveBeenCalledTimes(1);

    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });

    expect(mockedInvoke).toHaveBeenCalledTimes(2);
    expect(mocks.setUpdate).toHaveBeenCalledTimes(2);
    expect(mocks.setNotice).toHaveBeenCalledTimes(1);
  });

  it("keeps a displayed update and its Install button when a later check resolves null", async () => {
    mockedInvoke.mockResolvedValueOnce(info).mockResolvedValueOnce(null);

    renderHook(() => useUpdateCheck());
    await act(async () => {
      await Promise.resolve();
    });
    expect(mocks.setUpdate).toHaveBeenCalledTimes(1);
    expect(mocks.setUpdate).toHaveBeenCalledWith(info);
    expect(mocks.setNotice).toHaveBeenCalledTimes(1);

    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });

    // A quiet Rust failure is indistinguishable from "no update"; the store
    // keeps `info`, so the panel still shows the version and Install action.
    expect(mocks.setUpdate).toHaveBeenCalledTimes(1);
    expect(mocks.setUpdate).not.toHaveBeenCalledWith(null);
    expect(mocks.setNotice).toHaveBeenCalledTimes(1);
  });

  it("is silent on failure and retries on the next tick", async () => {
    mockedInvoke.mockRejectedValueOnce(new Error("network down"));

    renderHook(() => useUpdateCheck());
    await act(async () => {
      await Promise.resolve();
    });

    expect(mocks.setUpdate).not.toHaveBeenCalled();
    expect(mocks.setNotice).not.toHaveBeenCalled();

    mockedInvoke.mockResolvedValueOnce(info);
    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });

    expect(mockedInvoke).toHaveBeenCalledTimes(2);
    expect(mocks.setUpdate).toHaveBeenCalledWith(info);
    expect(mocks.setNotice).toHaveBeenCalledWith("Update available: 0.2.0");
  });

  it("announces a different version when a newer one appears", async () => {
    mockedInvoke.mockResolvedValueOnce(info).mockResolvedValueOnce({ ...info, version: "0.3.0" });

    renderHook(() => useUpdateCheck());
    await act(async () => {
      await Promise.resolve();
    });

    await act(async () => {
      vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS);
      await Promise.resolve();
    });

    expect(mocks.setNotice).toHaveBeenCalledTimes(2);
    expect(mocks.setNotice).toHaveBeenNthCalledWith(1, "Update available: 0.2.0");
    expect(mocks.setNotice).toHaveBeenNthCalledWith(2, "Update available: 0.3.0");
  });

  it("does not announce when no update is available", async () => {
    mockedInvoke.mockResolvedValue(null);

    renderHook(() => useUpdateCheck());
    await act(async () => {
      await Promise.resolve();
    });

    expect(mocks.setUpdate).toHaveBeenCalledWith(null);
    expect(mocks.setNotice).not.toHaveBeenCalled();
  });
});
