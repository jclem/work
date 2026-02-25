use std::path::PathBuf;
use std::sync::OnceLock;

static WORK_HOME: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Store the work home override from the CLI flag.
/// Falls through to `WORK_HOME` env var if `None`.
pub fn init(work_home: Option<PathBuf>) {
    let _ = WORK_HOME.set(work_home);
}

fn work_home() -> Option<PathBuf> {
    WORK_HOME.get().and_then(|p| p.clone()).or_else(|| {
        std::env::var("WORK_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    })
}

pub fn data_dir() -> Result<PathBuf, anyhow::Error> {
    if let Some(wp) = work_home() {
        return Ok(wp.join("data"));
    }

    let base = match std::env::var("XDG_DATA_HOME") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
            home.join(".local").join("share")
        }
    };

    Ok(base.join("work"))
}

pub fn runtime_dir() -> Result<PathBuf, anyhow::Error> {
    if let Some(wp) = work_home() {
        return Ok(wp.join("runtime"));
    }

    let base = match std::env::var("XDG_RUNTIME_DIR") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => std::env::temp_dir(),
    };

    Ok(base.join("work"))
}

pub fn config_dir() -> Result<PathBuf, anyhow::Error> {
    if let Some(wp) = work_home() {
        return Ok(wp.join("config"));
    }

    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
            home.join(".config")
        }
    };

    Ok(base.join("work"))
}

pub fn state_dir() -> Result<PathBuf, anyhow::Error> {
    if let Some(wp) = work_home() {
        return Ok(wp.join("state"));
    }

    let base = match std::env::var("XDG_STATE_HOME") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
            home.join(".local").join("state")
        }
    };

    Ok(base.join("work"))
}

pub fn task_log_dir() -> Result<PathBuf, anyhow::Error> {
    Ok(data_dir()?.join("logs").join("tasks"))
}

pub fn task_log_path(task_id: &str) -> Result<PathBuf, anyhow::Error> {
    Ok(task_log_dir()?.join(format!("{task_id}.log")))
}

pub fn tui_log_path() -> Result<PathBuf, anyhow::Error> {
    Ok(state_dir()?.join("tui.log"))
}

pub fn ensure_dirs() -> Result<(), anyhow::Error> {
    let data = data_dir()?;
    tracing::debug!(path = %data.display(), "ensuring data directory");
    std::fs::create_dir_all(data)?;

    let runtime = runtime_dir()?;
    tracing::debug!(path = %runtime.display(), "ensuring runtime directory");
    std::fs::create_dir_all(runtime)?;

    let state = state_dir()?;
    tracing::debug!(path = %state.display(), "ensuring state directory");
    std::fs::create_dir_all(state)?;

    Ok(())
}
