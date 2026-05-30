// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Integration test verifying that `--data-dir` routes store files to the specified directory.

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn data_dir_flag_routes_store_to_custom_path() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("kt")
        .unwrap()
        .args([
            "--data-dir",
            dir.path().to_str().unwrap(),
            "new",
            "test-task",
        ])
        .assert()
        .success();

    assert!(dir.path().join("taskset.json").exists());
}
