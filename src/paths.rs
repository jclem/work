use std::env;
use std::path::PathBuf;

const APP_DIR: &str = "work";
const SOCKET_FILE: &str = "workd.sock";
const PID_FILE: &str = "workd.pid";
const LOG_FILE: &str = "workd.log";
const DB_FILE: &str = "database.sqlite";

pub fn database_path() -> PathBuf {
    data_dir_root().join(DB_FILE)
}

pub fn pid_file_path() -> PathBuf {
    runtime_dir().join(PID_FILE)
}

pub fn daemon_log_path() -> PathBuf {
    runtime_dir().join(LOG_FILE)
}

pub fn socket_path(socket_override: Option<PathBuf>) -> PathBuf {
    if let Some(path) = socket_override {
        return path;
    }

    if let Ok(path) = env::var("WORKD_SOCKET_PATH")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }

    runtime_dir().join(SOCKET_FILE)
}

pub fn config_dir() -> PathBuf {
    if let Ok(path) = env::var("XDG_CONFIG_HOME")
        && !path.is_empty()
    {
        return PathBuf::from(path).join("work");
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("work")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn project_worktrees_dir(project_name: &str) -> PathBuf {
    data_dir_root()
        .join("projects")
        .join(project_name)
        .join("worktrees")
}

pub fn worktree_path(project_name: &str, task_name: &str) -> PathBuf {
    project_worktrees_dir(project_name).join(task_name)
}

pub fn projects_dir() -> PathBuf {
    data_dir_root().join("projects")
}

pub(crate) fn data_dir_root() -> PathBuf {
    if let Ok(path) = env::var("XDG_DATA_HOME")
        && !path.is_empty()
    {
        return PathBuf::from(path).join("work");
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share").join("work")
}

fn runtime_dir() -> PathBuf {
    if let Ok(path) = env::var("XDG_RUNTIME_DIR")
        && !path.is_empty()
    {
        return PathBuf::from(path).join(APP_DIR);
    }

    env::temp_dir().join(APP_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_uses_override() {
        let expected = PathBuf::from("/tmp/custom.sock");
        let actual = socket_path(Some(expected.clone()));
        assert_eq!(actual, expected);
    }
}
