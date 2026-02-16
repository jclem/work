use std::env;
use std::path::PathBuf;

const APP_DIR: &str = "workd";
const SOCKET_FILE: &str = "workd.sock";
const DB_FILE: &str = "database.sqlite";

pub fn database_path() -> PathBuf {
    data_dir().join(DB_FILE)
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

fn data_dir() -> PathBuf {
    if let Ok(path) = env::var("XDG_DATA_HOME")
        && !path.is_empty()
    {
        return PathBuf::from(path).join(APP_DIR);
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share").join(APP_DIR)
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
