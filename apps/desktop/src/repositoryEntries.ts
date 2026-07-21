export const INITIAL_REPOSITORY_ENTRY_LIMIT = 300;
export const REPOSITORY_ENTRY_PAGE_SIZE = 300;

type RepositoryEntryVisibility = {
  gitIgnored?: boolean;
  sensitive: string[];
};

export function visibleRepositoryEntries<T extends RepositoryEntryVisibility>(entries: T[]): T[] {
  return entries.filter((entry) => !entry.gitIgnored || entry.sensitive.length > 0);
}

export function nextRepositoryEntryLimit(current: number, total: number): number {
  return Math.min(current + REPOSITORY_ENTRY_PAGE_SIZE, total);
}
