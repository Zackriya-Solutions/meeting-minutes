import { describe, it, expect } from "vitest";
import { execSync } from "child_process";
import { join } from "path";

describe("check-i18n script", () => {
  it("exits 0 when all locales are complete", () => {
    const result = execSync("pnpm run check:i18n", {
      cwd: join(__dirname, "..", ".."),
      encoding: "utf-8",
    });
    expect(result).toContain("i18n check passed");
  });
});
