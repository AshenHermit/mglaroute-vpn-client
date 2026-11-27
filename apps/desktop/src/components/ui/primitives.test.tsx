import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { cn } from "../../lib/utils";
import { Button } from "./button";
import { Card } from "./card";

describe("token-based ui primitives", () => {
  it("button uses primary token, not sky palette", () => {
    render(<Button>Primary</Button>);
    const className = screen.getByRole("button", { name: "Primary" }).className;
    expect(className).toContain("bg-primary");
    expect(className).not.toContain("bg-sky-");
  });

  it("card uses card token", () => {
    render(<Card>content</Card>);
    expect(screen.getByText("content").className).toContain("bg-card");
  });

  it("cn resolves conflicting classes", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
  });
});
