export const MAX_RECENT_REPOSITORIES = 5;

export function addRecentRepository(repositories: string[], path: string): string[] {
  const normalizedPath = path.trim();
  if (!normalizedPath) return repositories;

  return [
    normalizedPath,
    ...repositories.filter((repository) => repository !== normalizedPath),
  ].slice(0, MAX_RECENT_REPOSITORIES);
}

export function removeRecentRepository(repositories: string[], path: string): string[] {
  return repositories.filter((repository) => repository !== path);
}

export function repositoryName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}
