import type { UpdateInfo } from "@mglaroute/contracts";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/invoke")>();
  return { ...actual, invoke: vi.fn() };
});

import { CommandInvocationError, invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";
import { UpdatePanel } from "./UpdatePanel";

const mockedInvoke = vi.mocked(invoke);

const installFailureNotice = "Update failed — VPN was disconnected; reconnect if needed";

const info: UpdateInfo = {
  version: "0.2.0",
  currentVersion: "0.1.0",
  notes: "Bug fixes and speed ups",
  pubDate: null,
  canInstallInApp: true,
  releaseUrl: "https://example.com/releases/v0.2.0",
};

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("UpdatePanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    act(() => {
      useAppStore.setState({
        update: null,
        updateInstalling: false,
        updateProgress: null,
        notice: null,
        noticeVariant: "info",
      });
    });
  });

  afterEach(cleanup);

  it("shows a calm message when no update is available", () => {
    render(<UpdatePanel />);

    expect(screen.getByText("No update available")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /install|release/i })).toBeNull();
  });

  it("shows the version and notes with an Install update button", () => {
    act(() => {
      useAppStore.setState({ update: info });
    });

    render(<UpdatePanel />);

    expect(screen.getByText(/0\.2\.0/)).toBeTruthy();
    expect(screen.getByText(info.notes ?? "")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Install update" })).toBeTruthy();
  });

  it("invokes update_install, sets installing synchronously and clears it after resolution", async () => {
    act(() => {
      useAppStore.setState({ update: info });
    });
    const pending = deferred<void>();
    mockedInvoke.mockReturnValueOnce(pending.promise as never);

    render(<UpdatePanel />);
    fireEvent.click(screen.getByRole("button", { name: "Install update" }));

    expect(mockedInvoke).toHaveBeenCalledWith("update_install", expect.anything());
    expect(useAppStore.getState().updateInstalling).toBe(true);

    await act(async () => {
      pending.resolve();
      await pending.promise;
    });

    expect(useAppStore.getState().updateInstalling).toBe(false);
  });

  it("opens the release page when in-app install is not available", async () => {
    act(() => {
      useAppStore.setState({ update: { ...info, canInstallInApp: false } });
    });
    const pending = deferred<void>();
    mockedInvoke.mockReturnValueOnce(pending.promise as never);

    render(<UpdatePanel />);
    fireEvent.click(screen.getByRole("button", { name: "Open release page" }));

    expect(mockedInvoke).toHaveBeenCalledWith("update_open_releases", expect.anything(), {
      url: info.releaseUrl,
    });
    expect(useAppStore.getState().updateInstalling).toBe(true);

    await act(async () => {
      pending.resolve();
      await pending.promise;
    });

    expect(useAppStore.getState().updateInstalling).toBe(false);
    expect(useAppStore.getState().notice).toBeNull();
  });

  it("keeps the generic notice when opening the release page fails", async () => {
    act(() => {
      useAppStore.setState({ update: { ...info, canInstallInApp: false } });
    });
    mockedInvoke.mockRejectedValueOnce(
      new CommandInvocationError("update_open_releases", "update"),
    );

    render(<UpdatePanel />);
    fireEvent.click(screen.getByRole("button", { name: "Open release page" }));

    await waitFor(() => expect(useAppStore.getState().notice).toBe("Update failed"));
    expect(useAppStore.getState().noticeVariant).toBe("error");
    expect(useAppStore.getState().updateInstalling).toBe(false);
  });

  it("install_button_disabled_while_installing", () => {
    act(() => {
      useAppStore.setState({ update: info, updateInstalling: true });
    });

    render(<UpdatePanel />);

    expect(screen.getByRole("button", { name: "Install update" }).hasAttribute("disabled")).toBe(
      true,
    );
  });

  it("renders a downloading progress line from the store", () => {
    act(() => {
      useAppStore.setState({
        update: info,
        updateProgress: { phase: "downloading", received: 1024, total: 2048 },
      });
    });

    render(<UpdatePanel />);

    expect(screen.getByText(/downloading/i)).toBeTruthy();
    expect(screen.getByText(/1024/)).toBeTruthy();
  });

  it("reports a disconnected-VPN notice on a rejected install without leaking native error text", async () => {
    act(() => {
      useAppStore.setState({ update: info });
    });
    mockedInvoke.mockRejectedValueOnce(new CommandInvocationError("update_install", "update"));

    render(<UpdatePanel />);
    fireEvent.click(screen.getByRole("button", { name: "Install update" }));

    await waitFor(() => expect(useAppStore.getState().updateInstalling).toBe(false));
    expect(useAppStore.getState().notice).toBe(installFailureNotice);
    expect(useAppStore.getState().noticeVariant).toBe("error");
    expect(screen.queryByText(/Tauri command/)).toBeNull();
  });

  it("reenables Install after a failed install even with a stale progress event", async () => {
    act(() => {
      useAppStore.setState({
        update: info,
        updateProgress: { phase: "downloading", received: 1024, total: null },
      });
    });
    mockedInvoke.mockRejectedValueOnce(new CommandInvocationError("update_install", "update"));

    render(<UpdatePanel />);
    const install = screen.getByRole("button", { name: "Install update" });
    fireEvent.click(install);

    await waitFor(() => expect(useAppStore.getState().notice).toBe(installFailureNotice));
    expect(useAppStore.getState().updateInstalling).toBe(false);
    expect(install.hasAttribute("disabled")).toBe(false);
  });
});
