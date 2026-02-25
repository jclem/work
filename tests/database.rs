mod common;

use std::time::{Duration, Instant};
use std::{path::Path, thread};

use predicates::prelude::*;

use common::DaemonFixture;

fn write_executable_script(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

fn wait_for_env_status(d: &DaemonFixture, env_id: &str, expected_status: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let env_list_out = d
            .assert_cmd()
            .args(["environment", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let envs: Vec<serde_json::Value> = serde_json::from_slice(&env_list_out).unwrap();
        if let Some(env) = envs
            .iter()
            .find(|candidate| candidate["id"].as_str() == Some(env_id))
            && env["status"].as_str() == Some(expected_status)
        {
            return;
        }

        if Instant::now() >= deadline {
            panic!("timed out waiting for environment {env_id} to become {expected_status}");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

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

#[test]
fn task_creation_failure_persists_failed_environment() {
    let d = DaemonFixture::start();

    let provider_script = d.work_dir.path().join("fail-env-provider.sh");
    std::fs::write(
        &provider_script,
        r#"#!/bin/sh
set -eu
action="$1"
case "$action" in
  prepare)
    exit 1
    ;;
  update|claim)
    echo '{}'
    ;;
  remove)
    exit 0
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&provider_script).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&provider_script, perms).unwrap();
    }

    let config_dir = d.work_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[tasks.providers.noop]
type = "command"
command = "sh"
args = ["-c", "true"]

[environments.providers.failing]
type = "script"
command = "{}"
"#,
            provider_script.to_string_lossy()
        ),
    )
    .unwrap();

    let proj = d.work_dir.path().join("task-proj");
    std::fs::create_dir(&proj).unwrap();
    d.assert_cmd()
        .args(["project", "new", "task-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let task_create = d
        .assert_cmd()
        .args([
            "task",
            "new",
            "should fail env prepare",
            "--project",
            "task-proj",
            "--provider",
            "noop",
            "--env-provider",
            "failing",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let task_json = String::from_utf8(task_create.get_output().stdout.clone()).unwrap();
    let task: serde_json::Value = serde_json::from_str(&task_json).unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let deadline = Instant::now() + Duration::from_secs(8);
    let (task_status, task_env_id, env_status) = loop {
        let task_list_out = d
            .assert_cmd()
            .args(["task", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let tasks: Vec<serde_json::Value> = serde_json::from_slice(&task_list_out).unwrap();
        let Some(task) = tasks
            .iter()
            .find(|candidate| candidate["id"].as_str() == Some(&task_id))
        else {
            if Instant::now() >= deadline {
                panic!("timed out waiting for task to appear");
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };

        let status = task["status"].as_str().unwrap_or_default().to_string();
        let env_id = task["environment_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        if status != "failed" || env_id.is_empty() {
            if Instant::now() >= deadline {
                panic!("timed out waiting for failed task with environment_id");
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        let env_list_out = d
            .assert_cmd()
            .args(["environment", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let envs: Vec<serde_json::Value> = serde_json::from_slice(&env_list_out).unwrap();
        let Some(env) = envs
            .iter()
            .find(|candidate| candidate["id"].as_str() == Some(env_id.as_str()))
        else {
            if Instant::now() >= deadline {
                panic!("timed out waiting for environment entry");
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };

        break (
            status,
            env_id,
            env["status"].as_str().unwrap_or_default().to_string(),
        );
    };

    assert_eq!(task_status, "failed");
    assert!(!task_env_id.is_empty());
    assert_eq!(env_status, "failed");
}

#[test]
fn environment_create_is_fully_async_and_eventually_claims() {
    let d = DaemonFixture::start();

    let provider_script = d.work_dir.path().join("slow-env-provider.sh");
    write_executable_script(
        &provider_script,
        r#"#!/bin/sh
set -eu
action="$1"
case "$action" in
  prepare)
    sleep 0.3
    echo '{}'
    ;;
  update|claim)
    echo '{}'
    ;;
  remove)
    exit 0
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let config_dir = d.work_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[environments.providers.slow]
type = "script"
command = "{}"
"#,
            provider_script.to_string_lossy()
        ),
    )
    .unwrap();

    let proj = d.work_dir.path().join("async-env-proj");
    std::fs::create_dir(&proj).unwrap();
    d.assert_cmd()
        .args(["project", "new", "async-env-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let create_out = d
        .assert_cmd()
        .args([
            "environment",
            "create",
            "async-env-proj",
            "--provider",
            "slow",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let env: serde_json::Value = serde_json::from_slice(&create_out).unwrap();
    let env_id = env["id"].as_str().unwrap().to_string();
    assert_eq!(env["status"], "preparing");

    wait_for_env_status(&d, &env_id, "in_use", Duration::from_secs(8));
}

#[test]
fn environment_prepare_writes_lifecycle_log_file() {
    let d = DaemonFixture::start();

    let provider_script = d.work_dir.path().join("logging-env-provider.sh");
    write_executable_script(
        &provider_script,
        r#"#!/bin/sh
set -eu
action="$1"
case "$action" in
  prepare|update|claim)
    echo "provider-output: action=$action" >&2
    echo '{}'
    ;;
  remove)
    exit 0
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let config_dir = d.work_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[environments.providers.logging]
type = "script"
command = "{}"
"#,
            provider_script.to_string_lossy()
        ),
    )
    .unwrap();

    let proj = d.work_dir.path().join("logging-env-proj");
    std::fs::create_dir(&proj).unwrap();
    d.assert_cmd()
        .args(["project", "new", "logging-env-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let prepare_out = d
        .assert_cmd()
        .args([
            "environment",
            "prepare",
            "logging-env-proj",
            "--provider",
            "logging",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let env: serde_json::Value = serde_json::from_slice(&prepare_out).unwrap();
    let env_id = env["id"].as_str().unwrap().to_string();

    wait_for_env_status(&d, &env_id, "pool", Duration::from_secs(8));

    let log_path = d
        .work_dir
        .path()
        .join("data/logs/environments")
        .join(format!("{env_id}.log"));
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let contents = std::fs::read_to_string(&log_path).unwrap_or_default();
        if contents.contains("job=prepare_environment")
            && contents.contains("phase=complete")
            && contents.contains("provider-output: action=prepare")
        {
            break;
        }

        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for lifecycle log content at {}: {}",
                log_path.display(),
                contents
            );
        }
        thread::sleep(Duration::from_millis(100));
    }

    d.assert_cmd()
        .args(["environment", "logs", &env_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("provider-output: action=prepare"));
}

#[test]
fn environment_update_is_queued_and_failure_happens_async() {
    let d = DaemonFixture::start();

    let provider_script = d.work_dir.path().join("failing-update-provider.sh");
    write_executable_script(
        &provider_script,
        r#"#!/bin/sh
set -eu
action="$1"
case "$action" in
  prepare)
    echo '{}'
    ;;
  update)
    exit 1
    ;;
  claim)
    echo '{}'
    ;;
  remove)
    exit 0
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let config_dir = d.work_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[environments.providers.updatefail]
type = "script"
command = "{}"
"#,
            provider_script.to_string_lossy()
        ),
    )
    .unwrap();

    let proj = d.work_dir.path().join("update-env-proj");
    std::fs::create_dir(&proj).unwrap();
    d.assert_cmd()
        .args(["project", "new", "update-env-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let prepare_out = d
        .assert_cmd()
        .args([
            "environment",
            "prepare",
            "update-env-proj",
            "--provider",
            "updatefail",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let env: serde_json::Value = serde_json::from_slice(&prepare_out).unwrap();
    let env_id = env["id"].as_str().unwrap().to_string();
    wait_for_env_status(&d, &env_id, "pool", Duration::from_secs(8));

    d.assert_cmd()
        .args(["environment", "update", &env_id, "--format", "json"])
        .assert()
        .success();

    wait_for_env_status(&d, &env_id, "failed", Duration::from_secs(8));
}

#[test]
fn task_remove_keeps_task_until_environment_cleanup_succeeds() {
    let d = DaemonFixture::start();
    let remove_fail_flag = d.work_dir.path().join("remove-fail.flag");
    std::fs::write(&remove_fail_flag, "1").unwrap();

    let provider_script = d.work_dir.path().join("failing-remove-provider.sh");
    write_executable_script(
        &provider_script,
        &format!(
            r#"#!/bin/sh
set -eu
action="$1"
fail_flag="{}"
case "$action" in
  prepare|update|claim)
    echo '{{}}'
    ;;
  remove)
    if [ -f "$fail_flag" ]; then
      exit 1
    fi
    exit 0
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
            remove_fail_flag.to_string_lossy()
        ),
    );

    let config_dir = d.work_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[tasks.providers.noop]
type = "command"
command = "sh"
args = ["-c", "true"]

[environments.providers.removefail]
type = "script"
command = "{}"
"#,
            provider_script.to_string_lossy()
        ),
    )
    .unwrap();

    let proj = d.work_dir.path().join("remove-task-proj");
    std::fs::create_dir(&proj).unwrap();
    d.assert_cmd()
        .args(["project", "new", "remove-task-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let task_create = d
        .assert_cmd()
        .args([
            "task",
            "new",
            "remove-failure-case",
            "--project",
            "remove-task-proj",
            "--provider",
            "noop",
            "--env-provider",
            "removefail",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task: serde_json::Value = serde_json::from_slice(&task_create).unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();
    let env_id = task["environment_id"].as_str().unwrap().to_string();

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let tasks_out = d
            .assert_cmd()
            .args(["task", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let tasks: Vec<serde_json::Value> = serde_json::from_slice(&tasks_out).unwrap();
        let status = tasks
            .iter()
            .find(|candidate| candidate["id"].as_str() == Some(task_id.as_str()))
            .and_then(|t| t["status"].as_str())
            .unwrap_or_default();
        if status == "complete" || status == "failed" {
            break;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for task completion before remove");
        }
        thread::sleep(Duration::from_millis(100));
    }

    d.assert_cmd()
        .args(["task", "logs", &task_id])
        .assert()
        .success();
    d.assert_cmd()
        .args(["task", "logs", &task_id, "--follow"])
        .assert()
        .success();

    d.assert_cmd()
        .args(["task", "remove", &task_id])
        .assert()
        .success();

    wait_for_env_status(&d, &env_id, "failed", Duration::from_secs(12));

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let tasks_out = d
            .assert_cmd()
            .args(["task", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let tasks: Vec<serde_json::Value> = serde_json::from_slice(&tasks_out).unwrap();
        if tasks
            .iter()
            .any(|candidate| candidate["id"].as_str() == Some(task_id.as_str()))
        {
            break;
        }
        if Instant::now() >= deadline {
            panic!("task was deleted even though environment removal failed");
        }
        thread::sleep(Duration::from_millis(100));
    }

    std::fs::remove_file(&remove_fail_flag).unwrap();

    d.assert_cmd()
        .args(["task", "remove", &task_id])
        .assert()
        .success();

    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let tasks_out = d
            .assert_cmd()
            .args(["task", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let tasks: Vec<serde_json::Value> = serde_json::from_slice(&tasks_out).unwrap();
        if tasks
            .iter()
            .all(|candidate| candidate["id"].as_str() != Some(task_id.as_str()))
        {
            break;
        }
        if Instant::now() >= deadline {
            panic!("task was not deleted after retrying removal");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn task_new_claims_pool_environment_and_task_remove_deletes_paired_environment() {
    let d = DaemonFixture::start();

    let provider_script = d.work_dir.path().join("pool-reuse-provider.sh");
    write_executable_script(
        &provider_script,
        r#"#!/bin/sh
set -eu
action="$1"
case "$action" in
  prepare)
    echo '{"worktree_path":"/tmp/fake","prepared":true}'
    ;;
  update|claim)
    echo '{}'
    ;;
  remove)
    exit 0
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let config_dir = d.work_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[tasks.providers.noop]
type = "command"
command = "sh"
args = ["-c", "true"]

[environments.providers.poolreuse]
type = "script"
command = "{}"
"#,
            provider_script.to_string_lossy()
        ),
    )
    .unwrap();

    let proj = d.work_dir.path().join("pool-reuse-proj");
    std::fs::create_dir(&proj).unwrap();
    d.assert_cmd()
        .args(["project", "new", "pool-reuse-proj", "--path"])
        .arg(&proj)
        .assert()
        .success();

    let prepare_out = d
        .assert_cmd()
        .args([
            "environment",
            "prepare",
            "pool-reuse-proj",
            "--provider",
            "poolreuse",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let prepared_env: serde_json::Value = serde_json::from_slice(&prepare_out).unwrap();
    let pool_env_id = prepared_env["id"].as_str().unwrap().to_string();
    wait_for_env_status(&d, &pool_env_id, "pool", Duration::from_secs(8));

    let task_out = d
        .assert_cmd()
        .args([
            "task",
            "new",
            "reuse pooled env",
            "--project",
            "pool-reuse-proj",
            "--provider",
            "noop",
            "--env-provider",
            "poolreuse",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task: serde_json::Value = serde_json::from_slice(&task_out).unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();
    let task_env_id = task["environment_id"].as_str().unwrap().to_string();
    assert_eq!(task_env_id, pool_env_id);

    wait_for_env_status(&d, &pool_env_id, "in_use", Duration::from_secs(8));

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let tasks_out = d
            .assert_cmd()
            .args(["task", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let tasks: Vec<serde_json::Value> = serde_json::from_slice(&tasks_out).unwrap();
        let status = tasks
            .iter()
            .find(|candidate| candidate["id"].as_str() == Some(task_id.as_str()))
            .and_then(|t| t["status"].as_str())
            .unwrap_or_default();
        if status == "complete" || status == "failed" {
            break;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for task completion");
        }
        thread::sleep(Duration::from_millis(100));
    }

    d.assert_cmd()
        .args(["task", "remove", &task_id])
        .assert()
        .success();

    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let tasks_out = d
            .assert_cmd()
            .args(["task", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let tasks: Vec<serde_json::Value> = serde_json::from_slice(&tasks_out).unwrap();
        let task_present = tasks
            .iter()
            .any(|candidate| candidate["id"].as_str() == Some(task_id.as_str()));

        let envs_out = d
            .assert_cmd()
            .args(["environment", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let envs: Vec<serde_json::Value> = serde_json::from_slice(&envs_out).unwrap();
        let env_present = envs
            .iter()
            .any(|candidate| candidate["id"].as_str() == Some(pool_env_id.as_str()));

        if !task_present && !env_present {
            break;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for task and paired environment removal");
        }
        thread::sleep(Duration::from_millis(100));
    }
}
