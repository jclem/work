mod common;

use predicates::prelude::*;

use common::DaemonFixture;

// --- Initialization ---

#[test]
fn initialize_creates_database() {
    let d = DaemonFixture::start();
    let db = d.work_dir.path().join("data/database.sqlite3");
    assert!(db.exists());
}

#[test]
fn work_home_flag_overrides_env() {
    let tmp = tempfile::TempDir::new().unwrap();

    // The daemon start uses WORK_HOME, so just verify via daemon.
    let d = common::DaemonFixture::start();
    // DB should be under the work dir's data/.
    assert!(d.work_dir.path().join("data/database.sqlite3").exists());
    drop(d);

    // Also verify --work-home flag works for non-daemon commands.
    // Without a daemon running, non-daemon commands will fail to connect,
    // but dirs should still be created.
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_work"));
    cmd.env_remove("WORK_HOME");
    cmd.env_remove("XDG_DATA_HOME");
    cmd.arg("--work-home").arg(tmp.path());
    let output = cmd.output().unwrap();
    // No-args command is a no-op now (no daemon needed), should succeed.
    assert!(output.status.success());
    assert!(tmp.path().join("data").exists());
    assert!(tmp.path().join("runtime").exists());
}

// --- Reset database ---

#[test]
fn reset_database_clears_data() {
    let d = DaemonFixture::start();
    let proj = d.work_dir.path().join("test-proj");
    std::fs::create_dir(&proj).unwrap();

    d.assert_cmd()
        .args(["project", "new", "test-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    d.assert_cmd().arg("reset-database").assert().success();

    d.assert_cmd()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// --- Project new ---

#[test]
fn project_new_creates_project_with_explicit_name_and_path() {
    let d = DaemonFixture::start();
    let project_path = d.work_dir.path().join("my-project");
    std::fs::create_dir(&project_path).unwrap();

    d.assert_cmd()
        .args(["project", "new", "my-project", "--path"])
        .arg(&project_path)
        .assert()
        .success();

    let assert = d
        .assert_cmd()
        .args(["project", "list", "--format", "json"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["name"], "my-project");
    assert_eq!(
        parsed[0]["path"],
        project_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
}

#[test]
fn project_new_defaults_name_to_directory_basename() {
    let d = DaemonFixture::start();
    let project_path = d.work_dir.path().join("cool-project");
    std::fs::create_dir(&project_path).unwrap();

    d.assert_cmd()
        .args(["project", "new", "--path"])
        .arg(&project_path)
        .assert()
        .success();

    let assert = d
        .assert_cmd()
        .args(["project", "list", "--format", "json"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed[0]["name"], "cool-project");
}

#[test]
fn project_new_defaults_path_to_current_dir() {
    let d = DaemonFixture::start();
    let project_dir = d.work_dir.path().join("from-cwd");
    std::fs::create_dir(&project_dir).unwrap();

    d.assert_cmd()
        .current_dir(&project_dir)
        .args(["project", "new"])
        .assert()
        .success();

    let assert = d
        .assert_cmd()
        .args(["project", "list", "--format", "json"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed[0]["name"], "from-cwd");
    assert_eq!(
        parsed[0]["path"],
        project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
}

#[test]
fn project_new_rejects_duplicate_name() {
    let d = DaemonFixture::start();
    let path_a = d.work_dir.path().join("a");
    let path_b = d.work_dir.path().join("b");
    std::fs::create_dir(&path_a).unwrap();
    std::fs::create_dir(&path_b).unwrap();

    d.assert_cmd()
        .args(["project", "new", "dupe", "--path"])
        .arg(&path_a)
        .assert()
        .success();

    d.assert_cmd()
        .args(["project", "new", "dupe", "--path"])
        .arg(&path_b)
        .assert()
        .failure();
}

// --- Project list ---

#[test]
fn project_list_empty() {
    let d = DaemonFixture::start();
    d.assert_cmd()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn project_list_human_format() {
    let d = DaemonFixture::start();
    let alpha = d.work_dir.path().join("alpha");
    let beta = d.work_dir.path().join("beta");
    std::fs::create_dir(&alpha).unwrap();
    std::fs::create_dir(&beta).unwrap();

    d.assert_cmd()
        .args(["project", "new", "beta", "--path"])
        .arg(&beta)
        .assert()
        .success();
    d.assert_cmd()
        .args(["project", "new", "alpha", "--path"])
        .arg(&alpha)
        .assert()
        .success();

    let assert = d.assert_cmd().args(["project", "list"]).assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3); // header + 2 projects
    assert!(lines[0].contains("NAME"));
    assert!(lines[0].contains("PATH"));
    assert!(lines[1].starts_with("alpha"));
    assert!(lines[2].starts_with("beta"));
}

#[test]
fn project_list_plain_format() {
    let d = DaemonFixture::start();
    let proj = d.work_dir.path().join("myproj");
    std::fs::create_dir(&proj).unwrap();

    d.assert_cmd()
        .args(["project", "new", "myproj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let assert = d
        .assert_cmd()
        .args(["project", "list", "--format", "plain"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1);
    let parts: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(parts[0], "myproj");
    assert_eq!(parts[1], proj.canonicalize().unwrap().to_string_lossy());
}

#[test]
fn project_list_json_format() {
    let d = DaemonFixture::start();
    let proj = d.work_dir.path().join("jsonproj");
    std::fs::create_dir(&proj).unwrap();

    d.assert_cmd()
        .args(["project", "new", "jsonproj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let assert = d
        .assert_cmd()
        .args(["project", "list", "--format", "json"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["name"], "jsonproj");
    assert_eq!(
        parsed[0]["path"],
        proj.canonicalize().unwrap().to_string_lossy().as_ref()
    );
}

#[test]
fn project_list_ls_alias() {
    let d = DaemonFixture::start();
    d.assert_cmd().args(["project", "ls"]).assert().success();
}

// --- Project remove ---

#[test]
fn project_remove_deletes_existing_project() {
    let d = DaemonFixture::start();
    let proj = d.work_dir.path().join("removeme");
    std::fs::create_dir(&proj).unwrap();

    d.assert_cmd()
        .args(["project", "new", "removeme", "--path"])
        .arg(&proj)
        .assert()
        .success();

    d.assert_cmd()
        .args(["project", "remove", "removeme"])
        .assert()
        .success();

    d.assert_cmd()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn project_remove_fails_for_nonexistent_project() {
    let d = DaemonFixture::start();

    d.assert_cmd()
        .args(["project", "rm", "nonexistent"])
        .assert()
        .failure();
}
