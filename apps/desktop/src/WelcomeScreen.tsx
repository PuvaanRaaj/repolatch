import { FolderOpen, FolderSimple } from "@phosphor-icons/react";
import { repositoryName } from "./recentRepositories";

const agentGuardIcon = new URL("../src-tauri/icons/icon.png", import.meta.url).href;
const isMac = navigator.userAgent.includes("Mac");

interface WelcomeScreenProps {
  notice?: string;
  opening: boolean;
  recentRepositories: string[];
  onOpen: () => void;
  onOpenRecent: (path: string) => void;
}

export function WelcomeScreen({
  notice,
  opening,
  recentRepositories,
  onOpen,
  onOpenRecent,
}: WelcomeScreenProps) {
  return (
    <main className="welcome-shell">
      <section className="welcome-panel" aria-labelledby="welcome-title">
        <div className="welcome-brand">
          <img src={agentGuardIcon} alt="" />
          <div>
            <h1 id="welcome-title">RepoLatch</h1>
            <p>A safe workspace for terminal coding agents.</p>
          </div>
        </div>

        <div className="welcome-group">
          <div className="welcome-heading">
            <span>Get started</span>
            <span aria-hidden="true" />
          </div>
          <button className="welcome-action" disabled={opening} onClick={onOpen}>
            <FolderOpen aria-hidden="true" size={19} weight="regular" />
            <span>{opening ? "Opening repository…" : "Open repository"}</span>
            <kbd>{isMac ? "⌘ O" : "Ctrl O"}</kbd>
          </button>
        </div>

        {recentRepositories.length > 0 && (
          <div className="welcome-group">
            <div className="welcome-heading">
              <span>Recent repositories</span>
              <span aria-hidden="true" />
            </div>
            <div className="recent-list">
              {recentRepositories.map((path, index) => (
                <button
                  className="welcome-action"
                  disabled={opening}
                  onClick={() => onOpenRecent(path)}
                  title={path}
                  key={path}
                >
                  <FolderSimple aria-hidden="true" size={18} weight="regular" />
                  <span>{repositoryName(path)}</span>
                  <kbd>{isMac ? "⌘" : "Alt"} {index + 1}</kbd>
                </button>
              ))}
            </div>
          </div>
        )}

        <p className="welcome-footnote">
          Native for speed. Container isolation when you need an enforced boundary.
        </p>
        {notice && <p className="welcome-error" role="alert">{notice}</p>}
      </section>
    </main>
  );
}
