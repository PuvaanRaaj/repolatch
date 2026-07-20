import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { isDangerousPolicyEdit, stateLabel, type EnforcementLevel } from "./status";
import "./styles.css";

type Entry = { path: string; kind: string; sizeBytes?: number; readAccess: string; writeAccess: string; sensitive: string[]; gitTracked?: boolean; gitIgnored?: boolean };
type Repository = { root: string; entries: Entry[]; warnings: string[]; git: { head?: string; dirty: boolean } };
type Capability = { level: EnforcementLevel; explanation: string };
type Backend = { id: string; available: boolean; availabilityMessage?: string; capabilities: Record<string, Capability> };
type Preview = { state: string; content?: string; message: string; bytes?: number };
type PolicyView = { source: string; valid: boolean; error?: string; allowedCommands: string[] };
type Session = { id: string; backend: string; workspace: string; diff: { changedPaths: string[]; linesAdded: number; linesRemoved: number }; receiptAvailable: boolean };

const call = <T,>(name: string, args?: Record<string, unknown>) => invoke<T>(name, args);
const backendName = (id: string) => id === "docker" ? "Container isolation" : "Native process";

function App() {
  const [page, setPage] = useState<"Repository" | "Policy" | "Sessions">("Repository");
  const [repo, setRepo] = useState<Repository>();
  const [backends, setBackends] = useState<Backend[]>([]);
  const [policy, setPolicy] = useState("");
  const [savedPolicy, setSavedPolicy] = useState("");
  const [allowedCommands, setAllowedCommands] = useState<string[]>([]);
  const [editing, setEditing] = useState(false);
  const [policyError, setPolicyError] = useState<string>();
  const [preview, setPreview] = useState<Preview>();
  const [session, setSession] = useState<Session>();
  const [diff, setDiff] = useState("");
  const [receipt, setReceipt] = useState<string>();
  const [notice, setNotice] = useState("Select a repository to begin.");

  const refreshBackends = () => call<Backend[]>("backend_status").then(setBackends).catch((error) => setNotice(String(error)));
  useEffect(() => { void refreshBackends(); }, []);

  const refreshRepo = async () => {
    try { setRepo(await call<Repository>("repository_tree")); setPreview(undefined); }
    catch (error) { setNotice(String(error)); }
  };
  const select = async () => {
    const selected = await call<string | null>("select_repository");
    if (selected) { setNotice(`Opened ${selected}`); await refreshRepo(); }
  };
  const openPolicy = async () => {
    try {
      const view = await call<PolicyView>("policy_load");
      setPolicy(view.source); setSavedPolicy(view.source); setAllowedCommands(view.allowedCommands);
      setPolicyError(view.error); setEditing(false); setPage("Policy");
    } catch (error) { setNotice(String(error)); }
  };
  const savePolicy = async () => {
    try {
      const view = await call<PolicyView>("policy_save", { source: policy, confirmEdit: true });
      setPolicy(view.source); setSavedPolicy(view.source); setAllowedCommands(view.allowedCommands);
      setEditing(false); setPolicyError(undefined); setNotice("Validated policy saved."); await refreshRepo();
    } catch (error) { setPolicyError(String(error)); }
  };
  const cancelPolicyEdit = () => { setPolicy(savedPolicy); setPolicyError(undefined); setEditing(false); };
  const showPreview = async (path: string) => {
    try { setPreview(await call<Preview>("file_preview", { relativePath: path })); }
    catch (error) { setNotice(String(error)); }
  };
  const launch = async (backend: string) => {
    if (backend === "local-advisory" && !window.confirm("Native execution is fast, but it runs as your macOS user and can access host files. Continue?")) return;
    const dockerImage = backend === "docker" ? window.prompt("Docker image containing the requested command:") : undefined;
    if (backend === "docker" && !dockerImage) return;
    const guidance = allowedCommands.length ? `Allowed commands:\n${allowedCommands.join("\n")}\n\nEnter argv, one argument per line:` : "Enter approved argv, one argument per line:";
    const raw = window.prompt(guidance, "cargo\ntest");
    if (!raw) return;
    try {
      setSession(await call<Session>("launch_workspace", { request: { backend, argv: raw.split("\n").filter(Boolean), dockerImage } }));
      setPage("Sessions"); setNotice("Session completed. Review changed paths, diff, and receipt.");
    } catch (error) { setNotice(`${String(error)} Check the Policy view for exact allowed commands.`); }
  };
  const loadSession = async () => {
    if (!session) return;
    try {
      setDiff(await call<string>("session_diff", { sessionId: session.id }));
      const view = await call<{ markdown: string }>("receipt_load", { sessionId: session.id });
      setReceipt(view.markdown);
    } catch (error) { setNotice(String(error)); }
  };

  const sensitive = repo?.entries.filter((entry) => entry.sensitive.length > 0).length ?? 0;
  return <main>
    <header><div><h1>AgentGuard</h1><p>Local workspace supervision</p></div><button onClick={select}>Open repository</button></header>
    <nav aria-label="Primary">{(["Repository", "Policy", "Sessions"] as const).map((item) => <button aria-current={page === item ? "page" : undefined} className={page === item ? "active" : ""} onClick={() => item === "Policy" ? openPolicy() : setPage(item)} key={item}>{item}</button>)}</nav>
    <p className="notice" role="status">{notice}</p>

    {page === "Repository" && <section className="layout"><article><h2>Repository</h2>{repo ? <><p className="muted">{repo.root}<br />Git {repo.git.head?.slice(0, 12) ?? "unborn"} · {repo.git.dirty ? "dirty" : "clean"} · {sensitive} sensitive or denied</p>{repo.warnings.map((warning) => <p className="warning" key={warning}>{warning}</p>)}<div className="tree">{repo.entries.filter((entry) => !entry.gitIgnored || entry.sensitive.length > 0).map((entry) => <button onClick={() => showPreview(entry.path)} className={entry.sensitive.length ? "sensitive" : ""} key={entry.path}><code>{entry.path}</code><small>{entry.kind} · read {entry.readAccess}{entry.sensitive.length ? " · withheld/masked" : ""}</small></button>)}</div></> : <p>No repository selected.</p>}</article><aside><h2>Preview</h2>{preview ? <><span className={`badge ${preview.state}`}>{preview.state}</span><p>{preview.message}</p>{preview.content && <pre>{preview.content}</pre>}</> : <p>Read-only. Binary, large, denied, and sensitive content is withheld.</p>}<h2>Execution</h2>{backends.map((backend) => <div className="backend" key={backend.id}><h3>{backendName(backend.id)} <span className={`badge ${backend.available ? (backend.id === "docker" ? "enforced" : "advisory") : "unavailable"}`}>{backend.available ? "Available" : "Unavailable"}</span></h3><p className="muted">{backend.id === "docker" ? "Optional enforced boundary. Uses a generated bind-mounted copy; slower on macOS." : "Fast path. No Docker. Filesystem and network controls are advisory."}</p>{backend.availabilityMessage && <p className="warning">{backend.availabilityMessage} Docker requests never fall back to native execution.</p>}{Object.entries(backend.capabilities).map(([name, value]) => <p key={name}><span className={`badge ${value.level}`}>{stateLabel(value.level)}</span> {name.replaceAll("_", " ")}: {value.explanation}</p>)}<button disabled={!backend.available} onClick={() => launch(backend.id)}>{backend.id === "docker" ? "Run in container" : "Run natively"}</button></div>)}</aside></section>}

    {page === "Policy" && <section><h2>Policy</h2><p>{isDangerousPolicyEdit(editing)} Deny rules win; unmatched paths remain invisible.</p><h3>Allowed commands</h3>{allowedCommands.length ? <ul className="commands">{allowedCommands.map((command) => <li key={command}><code>{command}</code></li>)}</ul> : <p className="muted">No allowed commands in the current valid policy.</p>}<textarea aria-label="AgentGuard policy" value={policy} readOnly={!editing} onChange={(event) => setPolicy(event.target.value)} />{policyError && <p className="warning">{policyError}</p>}<p>{editing ? <><button onClick={savePolicy}>Validate and save</button><button onClick={cancelPolicyEdit}>Cancel</button></> : <button onClick={() => setEditing(true)}>Edit policy</button>}</p></section>}

    {page === "Sessions" && <section><h2>Session review</h2>{session ? <><p><code>{session.id}</code> · {backendName(session.backend)} · +{session.diff.linesAdded} / -{session.diff.linesRemoved}</p><h3>Changed paths</h3>{session.diff.changedPaths.length ? <ul>{session.diff.changedPaths.map((path) => <li key={path}><code>{path}</code></li>)}</ul> : <p className="muted">No changed paths.</p>}<button onClick={loadSession}>Load diff and receipt</button>{diff && <><h3>Unified diff</h3><p className="warning">Diffs contain file content and may include secrets created during the session.</p><pre>{diff}</pre></>}{receipt && <><h3>Receipt</h3><pre>{receipt}</pre></>}</> : <p>No desktop session has completed yet.</p>}</section>}
  </main>;
}

createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
