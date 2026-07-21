import React, { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  addRecentRepository,
  MAX_RECENT_REPOSITORIES,
  removeRecentRepository,
  repositoryName,
} from "./recentRepositories";
import { WelcomeScreen } from "./WelcomeScreen";
import type { Backend, Preview, Repository, Session } from "./WorkspaceScreen";
import "./styles.css";

const WorkspaceScreen = lazy(async () => {
  const module = await import("./WorkspaceScreen");
  return { default: module.WorkspaceScreen };
});

type PolicyView = {
  source: string;
  valid: boolean;
  error?: string;
  allowedCommands: string[];
};

const RECENT_REPOSITORIES_KEY = "repolatch.recent-repositories.v1";
const call = <T,>(name: string, args?: Record<string, unknown>) => invoke<T>(name, args);

function loadRecentRepositories(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(RECENT_REPOSITORIES_KEY) ?? "[]");
    return Array.isArray(value)
      ? value.filter((path): path is string => typeof path === "string")
        .slice(0, MAX_RECENT_REPOSITORIES)
      : [];
  } catch {
    return [];
  }
}

function saveRecentRepositories(repositories: string[]) {
  try {
    localStorage.setItem(RECENT_REPOSITORIES_KEY, JSON.stringify(repositories));
  } catch {
    // Opening repositories does not depend on convenience history storage.
  }
}

function App() {
  const [repo, setRepo] = useState<Repository>();
  const [backends, setBackends] = useState<Backend[]>([]);
  const [policy, setPolicy] = useState("");
  const [savedPolicy, setSavedPolicy] = useState("");
  const [allowedCommands, setAllowedCommands] = useState<string[]>([]);
  const [editingPolicy, setEditingPolicy] = useState(false);
  const [policyError, setPolicyError] = useState<string>();
  const [session, setSession] = useState<Session>();
  const [diff, setDiff] = useState("");
  const [receipt, setReceipt] = useState<string>();
  const [notice, setNotice] = useState("");
  const [opening, setOpening] = useState(false);
  const [recentRepositories, setRecentRepositories] = useState(loadRecentRepositories);

  const rememberRepository = useCallback((path: string) => {
    setRecentRepositories((current) => {
      const next = addRecentRepository(current, path);
      saveRecentRepositories(next);
      return next;
    });
  }, []);

  const forgetRepository = useCallback((path: string) => {
    setRecentRepositories((current) => {
      const next = removeRecentRepository(current, path);
      saveRecentRepositories(next);
      return next;
    });
  }, []);

  const refreshBackends = useCallback(async () => {
    try {
      setBackends(await call<Backend[]>("backend_status"));
    } catch (error) {
      setNotice(String(error));
    }
  }, []);

  const refreshRepo = useCallback(async () => {
    setRepo(await call<Repository>("repository_tree"));
  }, []);

  const finishRepositoryOpen = useCallback(async (selected: string) => {
    setOpening(true);
    setNotice(`Scanning ${repositoryName(selected)}…`);
    try {
      await refreshRepo();
      rememberRepository(selected);
      setNotice(`Opened ${selected}`);
      void refreshBackends();
    } finally {
      setOpening(false);
    }
  }, [refreshBackends, refreshRepo, rememberRepository]);

  const select = useCallback(async () => {
    try {
      const path = await open({ directory: true, multiple: false, title: "Open repository" });
      if (!path) return;
      const selected = await call<string>("select_repository_path", { path });
      await finishRepositoryOpen(selected);
    } catch (error) {
      setOpening(false);
      setNotice(String(error));
    }
  }, [finishRepositoryOpen]);

  const selectRecent = useCallback(async (path: string) => {
    try {
      const selected = await call<string>("select_repository_path", { path });
      await finishRepositoryOpen(selected);
    } catch (error) {
      setOpening(false);
      forgetRepository(path);
      setNotice(`Could not open ${repositoryName(path)}. It was removed from recent repositories. ${String(error)}`);
    }
  }, [finishRepositoryOpen, forgetRepository]);

  useEffect(() => {
    if (repo) return;
    const handleShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void select();
        return;
      }
      const recentModifier = navigator.userAgent.includes("Mac") ? event.metaKey : event.altKey;
      const recentIndex = Number(event.key) - 1;
      if (recentModifier && recentIndex >= 0 && recentIndex < recentRepositories.length) {
        event.preventDefault();
        void selectRecent(recentRepositories[recentIndex]);
      }
    };
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [recentRepositories, repo, select, selectRecent]);

  const loadPolicy = useCallback(async () => {
    try {
      const view = await call<PolicyView>("policy_load");
      setPolicy(view.source);
      setSavedPolicy(view.source);
      setAllowedCommands(view.allowedCommands);
      setPolicyError(view.error);
      setEditingPolicy(false);
    } catch (error) {
      setNotice(String(error));
    }
  }, []);

  const savePolicy = async () => {
    try {
      const view = await call<PolicyView>("policy_save", { source: policy, confirmEdit: true });
      setPolicy(view.source);
      setSavedPolicy(view.source);
      setAllowedCommands(view.allowedCommands);
      setEditingPolicy(false);
      setPolicyError(undefined);
      setNotice("Validated policy saved.");
      await refreshRepo();
    } catch (error) {
      setPolicyError(String(error));
    }
  };

  const loadFile = (path: string) => call<Preview>("file_preview", { relativePath: path });

  const saveFile = async (path: string, content: string) => {
    const preview = await call<Preview>("file_save", {
      relativePath: path,
      content,
      confirmEdit: true,
    });
    await refreshRepo();
    return preview;
  };

  const launch = async (backend: string) => {
    if (backend === "local-advisory" && !window.confirm(
      "Native execution runs as your macOS user and can access host files. Continue?",
    )) return;
    const dockerImage = backend === "docker"
      ? window.prompt("Docker image containing the requested command:")
      : undefined;
    if (backend === "docker" && !dockerImage) return;
    const guidance = allowedCommands.length
      ? `Allowed commands:\n${allowedCommands.join("\n")}\n\nEnter argv, one argument per line:`
      : "Enter approved argv, one argument per line:";
    const raw = window.prompt(guidance, "cargo\ntest");
    if (!raw) return;
    try {
      setSession(await call<Session>("launch_workspace", {
        request: { backend, argv: raw.split("\n").filter(Boolean), dockerImage },
      }));
      setNotice("Session completed. Open Sessions to review its changed paths and receipt.");
    } catch (error) {
      setNotice(`${String(error)} Check the policy for exact allowed commands.`);
    }
  };

  const loadSession = async () => {
    if (!session) return;
    try {
      setDiff(await call<string>("session_diff", { sessionId: session.id }));
      const view = await call<{ markdown: string }>("receipt_load", { sessionId: session.id });
      setReceipt(view.markdown);
    } catch (error) {
      setNotice(String(error));
    }
  };

  if (!repo) {
    return (
      <WelcomeScreen
        notice={notice}
        opening={opening}
        recentRepositories={recentRepositories}
        onOpen={() => void select()}
        onOpenRecent={(path) => void selectRecent(path)}
      />
    );
  }

  return (
    <Suspense fallback={<main className="workspace-loading">Loading workspace…</main>}>
      <WorkspaceScreen
        repo={repo}
        backends={backends}
        notice={notice}
        policy={policy}
        savedPolicy={savedPolicy}
        policyError={policyError}
        allowedCommands={allowedCommands}
        editingPolicy={editingPolicy}
        session={session}
        diff={diff}
        receipt={receipt}
        onOpenAnother={() => void select()}
        onLoadFile={loadFile}
        onSaveFile={saveFile}
        onLoadPolicy={loadPolicy}
        onPolicyChange={setPolicy}
        onBeginPolicyEdit={() => setEditingPolicy(true)}
        onCancelPolicyEdit={() => {
          setPolicy(savedPolicy);
          setPolicyError(undefined);
          setEditingPolicy(false);
        }}
        onSavePolicy={savePolicy}
        onLaunch={launch}
        onLoadSession={loadSession}
      />
    </Suspense>
  );
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode><App /></React.StrictMode>,
);
