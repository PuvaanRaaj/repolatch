import { describe, expect, it } from "vitest";
import {
  INITIAL_REPOSITORY_ENTRY_LIMIT,
  nextRepositoryEntryLimit,
  visibleRepositoryEntries,
} from "./repositoryEntries";

describe("repository entry presentation", () => {
  it("hides ordinary ignored entries but keeps sensitive ones visible", () => {
    const entries = [
      { path: "src/main.ts", gitIgnored: false, sensitive: [] },
      { path: "dist/app.js", gitIgnored: true, sensitive: [] },
      { path: ".env", gitIgnored: true, sensitive: ["Environment"] },
    ];

    expect(visibleRepositoryEntries(entries).map((entry) => entry.path)).toEqual([
      "src/main.ts",
      ".env",
    ]);
  });

  it("reveals entries in bounded pages", () => {
    expect(nextRepositoryEntryLimit(INITIAL_REPOSITORY_ENTRY_LIMIT, 8425)).toBe(600);
    expect(nextRepositoryEntryLimit(600, 725)).toBe(725);
  });
});
