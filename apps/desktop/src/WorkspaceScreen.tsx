import { useCallback, useEffect, useMemo, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { StreamLanguage } from "@codemirror/language";
import { markdown as markdownLanguage } from "@codemirror/lang-markdown";
import { css } from "@codemirror/legacy-modes/mode/css";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { javascript } from "@codemirror/legacy-modes/mode/javascript";
import { python } from "@codemirror/legacy-modes/mode/python";
import { rust } from "@codemirror/legacy-modes/mode/rust";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { xml } from "@codemirror/legacy-modes/mode/xml";
import { yaml } from "@codemirror/legacy-modes/mode/yaml";
import {
  CaretDown,
  CaretRight,
  CheckCircle,
  File,
  FileCode,
  Files,
  FloppyDisk,
  Folder,
  FolderOpen,
  GitBranch,
  LockSimple,
  MagnifyingGlass,
  PencilSimple,
  Play,
  ShieldCheck,
  SlidersHorizontal,
  TerminalWindow,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import { repositoryName } from "./recentRepositories";
import { stateLabel, type EnforcementLevel } from "./status";
import {
  buildWorkspaceTree,
  fileName,
  parentPath,
  type RepositoryEntry,
  type TreeNode,
} from "./workspaceTree";

export type Repository = {
  root: string;
  entries: RepositoryEntry[];
  warnings: string[];
  git: { head?: string; dirty: boolean };
};

type Capability = { level: EnforcementLevel; explanation: string };
export type Backend = {
  id: string;
  available: boolean;
  availabilityMessage?: string;
  capabilities: Record<string, Capability>;
};

export type Preview = {
  state: string;
  content?: string;
  message: string;
  bytes?: number;
  editable: boolean;
};

export type Session = {
  id: string;
  backend: string;
  workspace: string;
  diff: { changedPaths: string[]; linesAdded: number; linesRemoved: number };
  receiptAvailable: boolean;
};

type OpenDocument = {
  path: string;
  content: string;
  savedContent: string;
  state: string;
  message: string;
  editable: boolean;
  editing: boolean;
};

type SidebarView = "explorer" | "guard" | "policy" | "sessions";

interface WorkspaceScreenProps {
  repo: Repository;
  backends: Backend[];
  notice: string;
  policy: string;
  savedPolicy: string;
  policyError?: string;
  allowedCommands: string[];
  editingPolicy: boolean;
  session?: Session;
  diff: string;
  receipt?: string;
  onOpenAnother: () => void;
  onLoadFile: (path: string) => Promise<Preview>;
  onSaveFile: (path: string, content: string) => Promise<Preview>;
  onLoadPolicy: () => Promise<void>;
  onPolicyChange: (source: string) => void;
  onBeginPolicyEdit: () => void;
  onCancelPolicyEdit: () => void;
  onSavePolicy: () => Promise<void>;
  onLaunch: (backend: string) => Promise<void>;
  onLoadSession: () => Promise<void>;
}

function backendName(id: string) {
  return id === "docker" ? "Container isolation" : "Native process";
}

function languageFor(path: string) {
  const name = fileName(path).toLowerCase();
  const extension = name.includes(".") ? name.split(".").pop() : "";
  if (["md", "mdx"].includes(extension ?? "")) return markdownLanguage();
  if (["js", "jsx", "ts", "tsx", "json"].includes(extension ?? "")) return StreamLanguage.define(javascript);
  if (extension === "py") return StreamLanguage.define(python);
  if (extension === "rs") return StreamLanguage.define(rust);
  if (extension === "toml") return StreamLanguage.define(toml);
  if (["yaml", "yml"].includes(extension ?? "")) return StreamLanguage.define(yaml);
  if (["html", "xml", "svg"].includes(extension ?? "")) return StreamLanguage.define(xml);
  if (["css", "scss", "sass", "less"].includes(extension ?? "")) return StreamLanguage.define(css);
  if (["sh", "bash", "zsh"].includes(extension ?? "")) return StreamLanguage.define(shell);
  if (name === "dockerfile" || name.startsWith("dockerfile.")) return StreamLanguage.define(dockerFile);
  return null;
}

function FileGlyph({ path }: { path: string }) {
  const code = /\.(rs|ts|tsx|js|jsx|py|go|java|php|rb|swift|css|html)$/i.test(path);
  const Icon = code ? FileCode : File;
  return <Icon aria-hidden="true" size={15} weight="regular" />;
}

interface TreeRowsProps {
  nodes: TreeNode[];
  depth?: number;
  expanded: Set<string>;
  activePath?: string;
  onToggle: (path: string) => void;
  onOpen: (entry: RepositoryEntry) => void;
}

function TreeRows({ nodes, depth = 0, expanded, activePath, onToggle, onOpen }: TreeRowsProps) {
  return nodes.map((node) => {
    const isExpanded = expanded.has(node.path);
    const isSensitive = (node.entry?.sensitive.length ?? 0) > 0;
    return (
      <div key={node.path}>
        <button
          className={`tree-row ${activePath === node.path ? "selected" : ""}`}
          style={{ paddingLeft: 8 + depth * 14 }}
          onClick={() => node.kind === "directory"
            ? onToggle(node.path)
            : node.entry && onOpen(node.entry)}
          title={node.path}
        >
          <span className="tree-chevron">
            {node.kind === "directory"
              ? isExpanded
                ? <CaretDown aria-hidden="true" size={12} weight="bold" />
                : <CaretRight aria-hidden="true" size={12} weight="bold" />
              : null}
          </span>
          {node.kind === "directory"
            ? isExpanded
              ? <FolderOpen aria-hidden="true" size={15} weight="regular" />
              : <Folder aria-hidden="true" size={15} weight="regular" />
            : <FileGlyph path={node.path} />}
          <span className="tree-label">{node.name}</span>
          {isSensitive && <span className="tree-lock"><LockSimple aria-label="Sensitive" size={12} /></span>}
        </button>
        {node.kind === "directory" && isExpanded && (
          <TreeRows
            nodes={node.children}
            depth={depth + 1}
            expanded={expanded}
            activePath={activePath}
            onToggle={onToggle}
            onOpen={onOpen}
          />
        )}
      </div>
    );
  });
}

function EmptyEditor() {
  return (
    <div className="editor-empty">
      <FileCode aria-hidden="true" size={42} weight="thin" />
      <p>Open a file from the explorer</p>
      <span>Files open read-only. Policy-approved files can be edited explicitly.</span>
    </div>
  );
}

export function WorkspaceScreen({
  repo,
  backends,
  notice,
  policy,
  savedPolicy,
  policyError,
  allowedCommands,
  editingPolicy,
  session,
  diff,
  receipt,
  onOpenAnother,
  onLoadFile,
  onSaveFile,
  onLoadPolicy,
  onPolicyChange,
  onBeginPolicyEdit,
  onCancelPolicyEdit,
  onSavePolicy,
  onLaunch,
  onLoadSession,
}: WorkspaceScreenProps) {
  const [sidebar, setSidebar] = useState<SidebarView>("explorer");
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [documents, setDocuments] = useState<OpenDocument[]>([]);
  const [activePath, setActivePath] = useState<string>();
  const [loadingPath, setLoadingPath] = useState<string>();
  const [status, setStatus] = useState("");
  const [cursor, setCursor] = useState({ line: 1, column: 1 });
  const visibleEntries = useMemo(
    () => repo.entries.filter((entry) => !entry.gitIgnored || entry.sensitive.length > 0),
    [repo.entries],
  );
  const tree = useMemo(() => buildWorkspaceTree(visibleEntries), [visibleEntries]);
  const activeDocument = documents.find((document) => document.path === activePath);
  const searchResults = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return [];
    return visibleEntries
      .filter((entry) => entry.kind === "file" && entry.path.toLowerCase().includes(normalized))
      .slice(0, 100);
  }, [query, visibleEntries]);

  const openFile = useCallback(async (entry: RepositoryEntry) => {
    if (entry.kind !== "file") return;
    if (documents.some((document) => document.path === entry.path)) {
      setActivePath(entry.path);
      return;
    }
    setLoadingPath(entry.path);
    setStatus(`Opening ${entry.path}…`);
    try {
      const preview = await onLoadFile(entry.path);
      const document: OpenDocument = {
        path: entry.path,
        content: preview.content ?? "",
        savedContent: preview.content ?? "",
        state: preview.state,
        message: preview.message,
        editable: preview.editable,
        editing: false,
      };
      setDocuments((current) => [...current, document]);
      setActivePath(entry.path);
      setStatus(preview.message);
      const parents = parentPath(entry.path).split("/").filter(Boolean);
      setExpanded((current) => {
        const next = new Set(current);
        let path = "";
        parents.forEach((part) => {
          path = path ? `${path}/${part}` : part;
          next.add(path);
        });
        return next;
      });
    } catch (error) {
      setStatus(String(error));
    } finally {
      setLoadingPath(undefined);
    }
  }, [documents, onLoadFile]);

  useEffect(() => {
    if (documents.length > 0 || loadingPath) return;
    const preferred = visibleEntries.find((entry) => entry.path.toLowerCase() === "readme.md")
      ?? visibleEntries.find((entry) => entry.kind === "file" && entry.readAccess === "allowed");
    if (preferred) void openFile(preferred);
  }, [documents.length, loadingPath, openFile, visibleEntries]);

  const updateDocument = (path: string, update: Partial<OpenDocument>) => {
    setDocuments((current) => current.map((document) =>
      document.path === path ? { ...document, ...update } : document));
  };

  const saveActive = useCallback(async () => {
    if (!activeDocument || !activeDocument.editing) return;
    setStatus(`Saving ${activeDocument.path}…`);
    try {
      const preview = await onSaveFile(activeDocument.path, activeDocument.content);
      updateDocument(activeDocument.path, {
        savedContent: preview.content ?? activeDocument.content,
        content: preview.content ?? activeDocument.content,
        message: preview.message,
        editable: preview.editable,
        editing: false,
      });
      setStatus(`Saved ${activeDocument.path}`);
    } catch (error) {
      setStatus(String(error));
    }
  }, [activeDocument, onSaveFile]);

  useEffect(() => {
    const handleSave = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void saveActive();
      }
    };
    window.addEventListener("keydown", handleSave);
    return () => window.removeEventListener("keydown", handleSave);
  }, [saveActive]);

  const closeDocument = (path: string) => {
    const document = documents.find((candidate) => candidate.path === path);
    if (document && document.content !== document.savedContent
      && !window.confirm(`Discard unsaved changes to ${fileName(path)}?`)) return;
    const index = documents.findIndex((candidate) => candidate.path === path);
    const next = documents.filter((candidate) => candidate.path !== path);
    setDocuments(next);
    if (activePath === path) setActivePath(next[Math.max(0, index - 1)]?.path);
  };

  const setSidebarView = (view: SidebarView) => {
    setSidebar(view);
    if (view === "policy") void onLoadPolicy();
  };

  const extensions = useMemo(() => {
    const language = languageFor(activeDocument?.path ?? "");
    return language ? [language] : [];
  }, [activeDocument?.path]);
  const displayStatus = status || notice || "Ready";

  return (
    <main className="ide-shell">
      <header className="ide-titlebar">
        <div className="project-crumb">
          <strong>{repositoryName(repo.root)}</strong>
          <span><GitBranch aria-hidden="true" size={13} /> {repo.git.head?.slice(0, 8) ?? "unborn"}</span>
          <span className={repo.git.dirty ? "dirty" : "clean"}>{repo.git.dirty ? "Modified" : "Clean"}</span>
        </div>
        <button className="titlebar-action" onClick={onOpenAnother}>
          <FolderOpen aria-hidden="true" size={15} /> Open repository
        </button>
      </header>

      <section className="ide-body">
        <nav className="activity-rail" aria-label="Workspace views">
          <button className={sidebar === "explorer" ? "active" : ""} onClick={() => setSidebarView("explorer")} title="Explorer">
            <Files aria-hidden="true" size={20} />
          </button>
          <button className={sidebar === "guard" ? "active" : ""} onClick={() => setSidebarView("guard")} title="Security and execution">
            <ShieldCheck aria-hidden="true" size={20} />
          </button>
          <button className={sidebar === "policy" ? "active" : ""} onClick={() => setSidebarView("policy")} title="Policy">
            <SlidersHorizontal aria-hidden="true" size={20} />
          </button>
          <button className={sidebar === "sessions" ? "active" : ""} onClick={() => setSidebarView("sessions")} title="Sessions">
            <TerminalWindow aria-hidden="true" size={20} />
          </button>
        </nav>

        <aside className="ide-sidebar">
          {sidebar === "explorer" && (
            <>
              <div className="sidebar-heading"><span>Explorer</span><small>{visibleEntries.length}</small></div>
              <label className="tree-search">
                <MagnifyingGlass aria-hidden="true" size={14} />
                <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Filter files" />
                {query && <button onClick={() => setQuery("")} aria-label="Clear filter"><X size={12} /></button>}
              </label>
              <div className="project-label"><CaretDown size={12} weight="bold" /> {repositoryName(repo.root)}</div>
              <div className="file-tree" aria-label="Repository files">
                {query ? searchResults.map((entry) => (
                  <button className={`tree-row search-result ${activePath === entry.path ? "selected" : ""}`} key={entry.path} onClick={() => void openFile(entry)}>
                    <FileGlyph path={entry.path} />
                    <span><strong>{fileName(entry.path)}</strong><small>{parentPath(entry.path)}</small></span>
                  </button>
                )) : (
                  <TreeRows
                    nodes={tree}
                    expanded={expanded}
                    activePath={activePath}
                    onToggle={(path) => setExpanded((current) => {
                      const next = new Set(current);
                      if (next.has(path)) next.delete(path); else next.add(path);
                      return next;
                    })}
                    onOpen={(entry) => void openFile(entry)}
                  />
                )}
                {loadingPath && <p className="sidebar-note">Opening {fileName(loadingPath)}…</p>}
              </div>
            </>
          )}

          {sidebar === "guard" && (
            <div className="sidebar-panel">
              <div className="sidebar-heading"><span>RepoLatch</span></div>
              <div className="guard-summary">
                <ShieldCheck size={22} weight="duotone" />
                <div><strong>Workspace controls</strong><span>{repo.entries.filter((entry) => entry.sensitive.length > 0).length} sensitive or denied paths</span></div>
              </div>
              {repo.warnings.map((warning) => <p className="compact-warning" key={warning}><WarningCircle size={14} />{warning}</p>)}
              {backends.map((backend) => (
                <section className="backend-card" key={backend.id}>
                  <div><strong>{backendName(backend.id)}</strong><span className={`mini-status ${backend.available ? (backend.id === "docker" ? "enforced" : "advisory") : "unavailable"}`}>{backend.available ? "Available" : "Unavailable"}</span></div>
                  <p>{backend.id === "docker" ? "Enforced generated-workspace boundary." : "Fast local execution with advisory host controls."}</p>
                  <button disabled={!backend.available} onClick={() => void onLaunch(backend.id)}><Play size={13} weight="fill" /> {backend.id === "docker" ? "Run isolated" : "Run natively"}</button>
                  <details>
                    <summary>Capability details</summary>
                    {Object.entries(backend.capabilities).map(([name, value]) => (
                      <p className="capability-row" key={name}><span className={`mini-status ${value.level}`}>{stateLabel(value.level)}</span>{name.replaceAll("_", " ")}</p>
                    ))}
                  </details>
                </section>
              ))}
            </div>
          )}

          {sidebar === "policy" && (
            <div className="sidebar-panel">
              <div className="sidebar-heading"><span>Policy</span></div>
              <p className="sidebar-copy">Deny rules win. File editing is available only where write access is explicitly allowed.</p>
              <h3>Allowed commands</h3>
              {allowedCommands.length ? allowedCommands.map((command) => <code className="command-chip" key={command}>{command}</code>) : <p className="sidebar-note">No commands allowed.</p>}
            </div>
          )}

          {sidebar === "sessions" && (
            <div className="sidebar-panel">
              <div className="sidebar-heading"><span>Sessions</span></div>
              {session ? (
                <button className="session-card" onClick={() => void onLoadSession()}>
                  <CheckCircle size={16} />
                  <span><strong>{backendName(session.backend)}</strong><small>+{session.diff.linesAdded} −{session.diff.linesRemoved} · {session.diff.changedPaths.length} files</small></span>
                </button>
              ) : <p className="sidebar-note">No session has completed in this app run.</p>}
            </div>
          )}
        </aside>

        <section className="workbench">
          {sidebar === "policy" ? (
            <div className="policy-workbench">
              <div className="editor-toolbar">
                <div><strong>repolatch.toml</strong><span>{editingPolicy ? "Editing policy" : "Read-only policy"}</span></div>
                <div>
                  {editingPolicy ? (
                    <><button onClick={onCancelPolicyEdit}>Cancel</button><button className="primary" onClick={() => void onSavePolicy()}><FloppyDisk size={14} /> Validate and save</button></>
                  ) : <button onClick={onBeginPolicyEdit}><PencilSimple size={14} /> Edit policy</button>}
                </div>
              </div>
              {policyError && <p className="editor-alert"><WarningCircle size={15} />{policyError}</p>}
              <CodeMirror value={policy || savedPolicy} onChange={onPolicyChange} readOnly={!editingPolicy} editable={editingPolicy} theme="dark" height="100%" extensions={[StreamLanguage.define(toml)]} basicSetup={{ foldGutter: true, highlightActiveLine: true }} />
            </div>
          ) : sidebar === "sessions" && session ? (
            <div className="session-workbench">
              <div className="editor-toolbar"><div><strong>Session review</strong><span>{session.id}</span></div><button onClick={() => void onLoadSession()}>Load diff and receipt</button></div>
              <div className="session-columns">
                <section><h2>Changed files</h2>{session.diff.changedPaths.length ? session.diff.changedPaths.map((path) => <code key={path}>{path}</code>) : <p>No changed paths.</p>}</section>
                <section><h2>Unified diff</h2><pre>{diff || "Load the session to render its diff."}</pre></section>
                <section><h2>Receipt</h2><pre>{receipt || "The receipt is available after loading this session."}</pre></section>
              </div>
            </div>
          ) : (
            <>
              <div className="editor-tabs" role="tablist">
                {documents.map((document) => (
                  <button className={document.path === activePath ? "active" : ""} onClick={() => setActivePath(document.path)} role="tab" aria-selected={document.path === activePath} key={document.path}>
                    <FileGlyph path={document.path} />
                    <span>{fileName(document.path)}</span>
                    {document.content !== document.savedContent && <i aria-label="Unsaved changes" />}
                    <span className="tab-close" role="button" aria-label={`Close ${fileName(document.path)}`} onClick={(event) => { event.stopPropagation(); closeDocument(document.path); }}><X size={12} /></span>
                  </button>
                ))}
              </div>
              {activeDocument ? (
                <div className="editor-pane">
                  <div className="editor-toolbar">
                    <div><strong>{activeDocument.path}</strong><span>{activeDocument.editing ? "Editing source file" : activeDocument.message}</span></div>
                    <div>
                      {activeDocument.editing ? (
                        <><button onClick={() => updateDocument(activeDocument.path, { content: activeDocument.savedContent, editing: false })}>Cancel</button><button className="primary" disabled={activeDocument.content === activeDocument.savedContent} onClick={() => void saveActive()}><FloppyDisk size={14} /> Save</button></>
                      ) : activeDocument.editable ? (
                        <button onClick={() => updateDocument(activeDocument.path, { editing: true })}><PencilSimple size={14} /> Edit file</button>
                      ) : <span className="read-only"><LockSimple size={13} /> Read only</span>}
                    </div>
                  </div>
                  {activeDocument.state === "available" ? (
                    <CodeMirror
                      value={activeDocument.content}
                      onChange={(content) => updateDocument(activeDocument.path, { content })}
                      onUpdate={(viewUpdate) => {
                        const head = viewUpdate.state.selection.main.head;
                        const line = viewUpdate.state.doc.lineAt(head);
                        setCursor({ line: line.number, column: head - line.from + 1 });
                      }}
                      readOnly={!activeDocument.editing}
                      editable={activeDocument.editing}
                      theme="dark"
                      height="100%"
                      extensions={extensions}
                      basicSetup={{ foldGutter: true, highlightActiveLine: true, highlightSelectionMatches: true }}
                    />
                  ) : (
                    <div className="withheld-editor"><LockSimple size={34} weight="thin" /><strong>{activeDocument.state === "masked" ? "Masked preview" : "Content withheld"}</strong><p>{activeDocument.message}</p>{activeDocument.content && <pre>{activeDocument.content}</pre>}</div>
                  )}
                </div>
              ) : <EmptyEditor />}
            </>
          )}
        </section>
      </section>

      <footer className="statusbar">
        <span><ShieldCheck size={13} weight="fill" /> RepoLatch active</span>
        <span className="status-message">{displayStatus}</span>
        {activeDocument && <><span>Ln {cursor.line}, Col {cursor.column}</span><span>{activeDocument.editing ? "Writable" : "Read-only"}</span></>}
      </footer>
    </main>
  );
}
