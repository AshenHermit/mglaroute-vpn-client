import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Dialog } from "./dialog";
import { Switch } from "./switch";

describe("ui primitives", () => {
  it("radix_switch_and_dialog_wrappers", () => {
    const onCheckedChange = vi.fn();
    render(
      <Switch
        checked
        onCheckedChange={onCheckedChange}
        label="Auto refetch enabled"
      />,
    );

    const sw = screen.getByRole("switch", { name: "Auto refetch enabled" });
    expect(sw.getAttribute("data-state")).toBe("checked");
    fireEvent.click(sw);
    expect(onCheckedChange).toHaveBeenCalledWith(false);

    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} title="Edit profile">
        <p>body</p>
      </Dialog>,
    );

    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("data-state")).toBe("open");
    expect(screen.getByText("Edit profile")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(onClose).toHaveBeenCalled();
  });
});
