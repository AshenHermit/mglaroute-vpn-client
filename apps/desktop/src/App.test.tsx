import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./lib/invoke", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/invoke")>();
  return {
    ...actual,
    invoke: vi.fn(async (command: string) =>
      command === "anti_dpi_status" ? { status: "unsupported" } : [],
    ),
  };
});
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import App from "./App";

describe("App", () => {
  afterEach(cleanup);

  it("renders the mglaroute heading and all three shell zones", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "mglaroute" })).toBeTruthy();
    expect(screen.getByTestId("rail-zone")).toBeTruthy();
    expect(screen.getByTestId("center-zone")).toBeTruthy();
    expect(screen.getByTestId("right-zone")).toBeTruthy();
  });
});
