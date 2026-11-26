import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "../stores/app.store";
import { NoticeToaster } from "./notice-toaster";

vi.mock("sonner", () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    loading: vi.fn(),
  },
}));

import { toast } from "sonner";

const toasts = vi.mocked(toast);

describe("NoticeToaster", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({ notice: null, noticeSeq: 0, noticeVariant: "info" });
  });

  afterEach(() => {
    cleanup();
    useAppStore.setState({ notice: null, noticeSeq: 0, noticeVariant: "info" });
  });

  it("shows a success notice through sonner with the right message", async () => {
    render(<NoticeToaster />);

    act(() => {
      useAppStore.getState().setNotice("Saved", "success");
    });

    await waitFor(() => expect(toasts.success).toHaveBeenCalledWith("Saved"));
  });

  it("shows an error notice through sonner with the right message", async () => {
    render(<NoticeToaster />);

    act(() => {
      useAppStore.getState().setNotice("Failed", "error");
    });

    await waitFor(() => expect(toasts.error).toHaveBeenCalledWith("Failed"));
  });

  it("shows the same message again when the notice sequence advances", async () => {
    render(<NoticeToaster />);

    act(() => {
      useAppStore.getState().setNotice("Refresh complete");
    });
    await waitFor(() => expect(toasts.info).toHaveBeenCalledTimes(1));

    act(() => {
      useAppStore.getState().setNotice("Refresh complete");
    });
    await waitFor(() => expect(toasts.info).toHaveBeenCalledTimes(2));
  });
});
