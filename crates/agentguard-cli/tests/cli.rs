use std::{fs, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("agentguard").expect("binary")
}

fn repository() -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(temp.path())
        .status()
        .expect("git")
        .success()
        .then_some(())
        .expect("git init succeeds");
    fs::write(temp.path().join("safe file 你好.txt"), "safe\n").expect("safe fixture");
    fs::write(temp.path().join(".env"), "FAKE_SECRET_DO_NOT_PRINT").expect("secret fixture");
    fs::write(
        temp.path().join("agentguard.toml"),
        r#"version = 1
[filesystem]
read = ["**"]
write = ["**"]
deny = [".env", ".env.*"]
[network]
mode = "deny"
[network.allow]
hosts = []
[commands]
allow = ["sh -c printf generated > output.txt", "codex"]
"#,
    )
    .expect("policy fixture");
    temp
}

#[test]
fn init_never_overwrites_a_policy() {
    let temp = TempDir::new().expect("tempdir");
    bin()
        .args(["init", "--repo"])
        .arg(temp.path())
        .assert()
        .success();
    bin()
        .args(["init", "--repo"])
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to overwrite"));
}

#[test]
fn malformed_policy_fails_validation() {
    let temp = TempDir::new().expect("tempdir");
    fs::write(temp.path().join("agentguard.toml"), "version = 99").expect("policy");
    bin()
        .args(["policy", "validate", "--repo"])
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid agentguard.toml"));
}

#[test]
fn inspect_is_metadata_only_for_secret_paths() {
    let repo = repository();
    bin()
        .args(["inspect", "--repo"])
        .arg(repo.path())
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains(".env"))
        .stdout(predicate::str::contains("FAKE_SECRET_DO_NOT_PRINT").not());
}

#[test]
fn workspace_and_local_run_leave_source_isolated_and_receipted() {
    let repo = repository();
    let output = TempDir::new().expect("output");
    let workspace = output.path().join("workspace with spaces 你好");
    let receipt = output.path().join("receipt.json");
    bin()
        .args(["workspace", "create", "--repo"])
        .arg(repo.path())
        .arg("--output")
        .arg(&workspace)
        .assert()
        .success();
    assert!(workspace.join("safe file 你好.txt").is_file());
    assert!(!workspace.join(".env").exists());
    assert_eq!(
        fs::read_to_string(repo.path().join(".env")).unwrap(),
        "FAKE_SECRET_DO_NOT_PRINT"
    );

    let run_workspace = output.path().join("run workspace");
    bin()
        .args(["run", "--repo"])
        .arg(repo.path())
        .args(["--backend", "local", "--workspace"])
        .arg(&run_workspace)
        .arg("--receipt")
        .arg(&receipt)
        .arg("--")
        .args(["sh", "-c", "printf generated > output.txt"])
        .assert()
        .success()
        .stderr(predicate::str::contains("LOCAL ADVISORY"));
    assert!(run_workspace.join("output.txt").is_file());
    assert!(!repo.path().join("output.txt").exists());
    bin()
        .args(["diff", "--workspace"])
        .arg(&run_workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("output.txt"));
    bin()
        .args(["receipt", "--path"])
        .arg(&receipt)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("completed"));
}

#[test]
fn docker_requires_image_and_never_falls_back() {
    let repo = repository();
    bin()
        .args(["run", "--repo"])
        .arg(repo.path())
        .args(["--backend", "docker"])
        .args(["--agent", "codex"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires an explicit --image"));
}

#[test]
#[ignore = "requires a reachable Docker daemon and alpine:latest"]
fn docker_e2e_isolation() {
    let repository = TempDir::new().expect("repository");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/docker-e2e");
    for name in ["agentguard.toml", "safe.txt", ".env"] {
        fs::copy(fixture.join(name), repository.path().join(name)).expect("copy fixture");
    }
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repository.path())
            .status()
            .expect("git")
            .success()
    );
    let output = TempDir::new().expect("output parent");
    let workspace = output.path().join("workspace");
    let receipt = output.path().join("receipt.json");
    let script = "if wget -q -T 3 -O- http://example.com; then exit 90; fi; test ! -e .env; printf docker-ok > docker-output.txt";
    bin()
        .args(["run", "--repo"])
        .arg(repository.path())
        .args(["--backend", "docker", "--image", "alpine:latest"])
        .arg("--workspace")
        .arg(&workspace)
        .arg("--receipt")
        .arg(&receipt)
        .args(["--", "sh", "-c", script])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(workspace.join("docker-output.txt")).unwrap(),
        "docker-ok"
    );
    assert!(!workspace.join(".env").exists());
    assert!(!repository.path().join("docker-output.txt").exists());
    let saved = fs::read_to_string(receipt).unwrap();
    assert!(saved.contains("\"status\": \"completed\""));
    assert!(!saved.contains("must-not-appear"));
}
