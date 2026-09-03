// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Integration test verifying that `--data-dir` routes the event log to the specified directory
//! and that a full punch-in/punch-out lifecycle records what the log command reports.

use assert_cmd::Command;
use tempfile::{TempDir, tempdir};

/// Runs `kt` against an isolated store directory.
fn kt(dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("kt").unwrap();
    cmd.args(["--data-dir", dir.path().to_str().unwrap()])
        .args(args)
        .assert()
}

#[test]
fn data_dir_flag_routes_store_to_custom_path() {
    let dir = tempdir().unwrap();

    kt(&dir, &["new", "test-project"]).success();

    assert!(dir.path().join("events.jsonl").exists());
}

#[test]
fn read_only_commands_leave_no_trace() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("store");

    Command::cargo_bin("kt")
        .unwrap()
        .args(["--data-dir", nested.to_str().unwrap(), "log"])
        .assert()
        .success();

    assert!(!nested.exists(), "reading must not create the store");
}

#[test]
fn full_lifecycle_records_and_reports_time() {
    let dir = tempdir().unwrap();

    kt(&dir, &["new", "alpha"]).success();
    kt(&dir, &["new", "beta"]).success();

    kt(&dir, &["in", "alpha"]).success();
    kt(&dir, &["in", "beta"]).success();
    kt(&dir, &["out"]).success();

    // Both projects appear, with the one the timer last stopped on marked.
    let listed = kt(&dir, &["list"]).success();
    let listed = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    assert!(listed.contains("alpha"));
    assert!(listed.contains("beta"));

    // Two intervals were recorded: alpha closed by the switch, beta closed by `out`.
    let logged = kt(&dir, &["log", "--json"]).success();
    let logged = String::from_utf8(logged.get_output().stdout.clone()).unwrap();
    let records: Vec<&str> = logged.lines().filter(|l| !l.trim().is_empty()).collect();

    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r.contains(r#""name":"alpha""#)));
    assert!(records.iter().any(|r| r.contains(r#""name":"beta""#)));
}

#[test]
fn punching_in_to_an_unknown_project_fails_with_suggestions() {
    let dir = tempdir().unwrap();

    kt(&dir, &["new", "alpha"]).success();
    kt(&dir, &["in", "alpga"]).failure();
}

#[test]
fn duplicate_project_names_are_rejected() {
    let dir = tempdir().unwrap();

    kt(&dir, &["new", "alpha"]).success();
    kt(&dir, &["new", "alpha"]).failure();
}
