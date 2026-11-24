/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["packages/contracts/src/**/*.test.ts", "apps/desktop/src/lib/**/*.test.ts"],
  },
});
