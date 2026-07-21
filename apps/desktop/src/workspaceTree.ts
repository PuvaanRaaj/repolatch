export type RepositoryEntry = {
  path: string;
  kind: string;
  sizeBytes?: number;
  readAccess: string;
  writeAccess: string;
  sensitive: string[];
  gitTracked?: boolean;
  gitIgnored?: boolean;
};

export type TreeNode = {
  name: string;
  path: string;
  kind: "directory" | "file";
  entry?: RepositoryEntry;
  children: TreeNode[];
};

function compareNodes(left: TreeNode, right: TreeNode) {
  if (left.kind !== right.kind) return left.kind === "directory" ? -1 : 1;
  return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
}

export function buildWorkspaceTree(entries: RepositoryEntry[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const nodes = new Map<string, TreeNode>();

  for (const entry of entries) {
    const parts = entry.path.split("/").filter(Boolean);
    let parentChildren = roots;
    let currentPath = "";

    parts.forEach((part, index) => {
      currentPath = currentPath ? `${currentPath}/${part}` : part;
      const isLeaf = index === parts.length - 1;
      let node = nodes.get(currentPath);
      if (!node) {
        node = {
          name: part,
          path: currentPath,
          kind: isLeaf && entry.kind !== "directory" ? "file" : "directory",
          entry: isLeaf ? entry : undefined,
          children: [],
        };
        nodes.set(currentPath, node);
        parentChildren.push(node);
      } else if (isLeaf) {
        node.entry = entry;
        node.kind = entry.kind === "directory" ? "directory" : "file";
      }
      parentChildren = node.children;
    });
  }

  const sort = (children: TreeNode[]) => {
    children.sort(compareNodes);
    children.forEach((node) => sort(node.children));
  };
  sort(roots);
  return roots;
}

export function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}

export function parentPath(path: string): string {
  const parts = path.split("/");
  parts.pop();
  return parts.join("/");
}
