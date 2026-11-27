import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ImageBackground } from "./ImageBackground";

describe("ImageBackground", () => {
  afterEach(cleanup);

  it("renders a bundled parallax image", () => {
    render(<ImageBackground />);
    const bg = screen.getByTestId("app-background");

    expect(bg.style.backgroundImage).toMatch(/\/bg\/bg_[1-3]\.jpg/);
  });

  it("shifts the background on pointer move and center-zone scroll", async () => {
    render(
      <>
        <div data-testid="center-zone" />
        <ImageBackground />
      </>,
    );
    const bg = screen.getByTestId("app-background");
    const zone = screen.getByTestId("center-zone");

    fireEvent(window, new MouseEvent("pointermove", { clientX: 10, clientY: 20 }));
    await waitFor(() => expect(bg.style.transform).toContain("translate3d"));
    const afterPointer = bg.style.transform;

    Object.defineProperty(zone, "scrollTop", { value: 120, configurable: true });
    fireEvent.scroll(zone);
    await waitFor(() => expect(bg.style.transform).not.toBe(afterPointer));
  });
});
