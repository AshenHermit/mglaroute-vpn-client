/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["**/*.test.{ts,tsx,mjs}"],
    exclude: ["**/node_modules/**", "**/.worktrees/**"],
  },
});
