import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("sonner", () => ({
  Toaster: (props: { mobileOffset?: { bottom?: string } }) => (
    <div
      data-testid="sonner-toaster"
      data-mobile-offset={JSON.stringify(props.mobileOffset ?? {})}
    />
  ),
}));

import { Toaster } from "./sonner";

describe("sonner Toaster wrapper", () => {
  afterEach(cleanup);

  it("keeps mobile toasts above the portrait bottom navigation", () => {
    render(<Toaster />);

    const raw = screen.getByTestId("sonner-toaster").getAttribute("data-mobile-offset");
    const offset = JSON.parse(raw ?? "{}") as { bottom?: string };
    expect(offset.bottom).toContain("72px");
  });
});
