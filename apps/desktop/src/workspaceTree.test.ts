import { describe, expect, it } from "vitest";
import { buildWorkspaceTree, fileName, type RepositoryEntry } from "./workspaceTree";

const entry = (path: string, kind = "file"): RepositoryEntry => ({
  path,
  kind,
  readAccess: "allowed",
  writeAccess: "allowed",
  sensitive: [],
});

describe("buildWorkspaceTree", () => {
  it("creates missing parent folders and sorts folders before files", () => {
    const tree = buildWorkspaceTree([
      entry("README.md"),
      entry("src/main.rs"),
      entry("src", "directory"),
      entry("Cargo.toml"),
    ]);

    expect(tree.map((node) => node.path)).toEqual(["src", "Cargo.toml", "README.md"]);
    expect(tree[0].children.map((node) => node.path)).toEqual(["src/main.rs"]);
  });

  it("returns a compact display name for tabs", () => {
    expect(fileName("apps/desktop/src/main.tsx")).toBe("main.tsx");
  });
});
