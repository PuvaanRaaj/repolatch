import { describe, expect, it } from "vitest";
import {
  addRecentRepository,
  MAX_RECENT_REPOSITORIES,
  removeRecentRepository,
  repositoryName,
} from "./recentRepositories";

describe("recent repositories", () => {
  it("moves the latest repository to the front without duplicates", () => {
    expect(addRecentRepository(["/work/api", "/work/web"], "/work/web")).toEqual([
      "/work/web",
      "/work/api",
    ]);
  });

  it("keeps only the most recent repositories", () => {
    const repositories = Array.from(
      { length: MAX_RECENT_REPOSITORIES },
      (_, index) => `/work/repository-${index}`,
    );

    expect(addRecentRepository(repositories, "/work/latest")).toHaveLength(
      MAX_RECENT_REPOSITORIES,
    );
    expect(addRecentRepository(repositories, "/work/latest")[0]).toBe("/work/latest");
  });

  it("removes unavailable repositories", () => {
    expect(removeRecentRepository(["/work/api", "/work/web"], "/work/api")).toEqual([
      "/work/web",
    ]);
  });

  it("uses the final path segment as the display name", () => {
    expect(repositoryName("/work/fiuu/api")).toBe("api");
    expect(repositoryName("C:\\work\\repolatch")).toBe("repolatch");
  });
});
