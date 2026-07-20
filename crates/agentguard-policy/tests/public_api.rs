use std::fs;

use agentguard_core::RepoRoot;
use agentguard_policy::{
    Access, AccessDecision, DEFAULT_POLICY_TEMPLATE, ScanOptions, compile_policy, scan_repository,
};
use tempfile::tempdir;

#[test]
fn public_api_compiles_and_scans_without_exposing_file_contents() {
    let directory = tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/lib.rs"), "pub fn example() {}\n").unwrap();
    fs::write(directory.path().join(".env"), "FAKE_SECRET=never-returned").unwrap();

    let policy = compile_policy(DEFAULT_POLICY_TEMPLATE).unwrap();
    let root = RepoRoot::discover(directory.path()).unwrap();
    let scan = scan_repository(
        &root,
        &policy,
        ScanOptions {
            annotate_git: false,
            ..ScanOptions::default()
        },
    );

    let source = scan
        .entries
        .iter()
        .find(|entry| entry.path.as_str() == "src/lib.rs")
        .unwrap();
    let environment = scan
        .entries
        .iter()
        .find(|entry| entry.path.as_str() == ".env")
        .unwrap();
    assert_eq!(source.read_access, AccessDecision::Allowed);
    assert_eq!(environment.read_access, AccessDecision::Denied);
    assert!(
        environment
            .sensitive
            .iter()
            .any(|match_| !match_.pattern.is_empty())
    );
    assert_eq!(
        policy.evaluate(&environment.path, Access::Read),
        AccessDecision::Denied
    );
}
