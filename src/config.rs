use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::CliError;
use crate::paths;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub projects: Option<HashMap<String, ProjectConfig>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectConfig {
    pub hooks: Option<HooksConfig>,
}

#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    #[serde(rename = "new-after")]
    pub new_after: Option<String>,
}

pub fn load() -> Result<Config, CliError> {
    let path = paths::config_path();

    if !path.exists() {
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|source| {
        CliError::with_source(format!("failed to read config file: {}", path.display()), source)
    })?;

    let config: Config = toml::from_str(&content).map_err(|source| {
        CliError::with_source(format!("failed to parse config file: {}", path.display()), source)
    })?;

    Ok(config)
}

pub fn load_project_config(project_path: &str) -> Result<ProjectConfig, CliError> {
    let path = Path::new(project_path).join(".work/config.toml");

    if !path.exists() {
        return Ok(ProjectConfig::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|source| {
        CliError::with_source(format!("failed to read project config: {}", path.display()), source)
    })?;

    let config: ProjectConfig = toml::from_str(&content).map_err(|source| {
        CliError::with_source(format!("failed to parse project config: {}", path.display()), source)
    })?;

    Ok(config)
}

pub fn hook_script<'a>(config: &'a Config, project_name: &str, hook_name: &str) -> Option<&'a str> {
    let projects = config.projects.as_ref()?;
    let project = projects.get(project_name)?;
    project_hook_script(project, hook_name)
}

pub fn project_hook_script<'a>(project: &'a ProjectConfig, hook_name: &str) -> Option<&'a str> {
    let hooks = project.hooks.as_ref()?;

    match hook_name {
        "new-after" => hooks.new_after.as_deref(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_default_when_file_missing() {
        let config = Config::default();
        assert!(config.projects.is_none());
    }

    #[test]
    fn hook_script_returns_none_for_missing_project() {
        let config = Config::default();
        assert!(hook_script(&config, "no-such-project", "new-after").is_none());
    }

    #[test]
    fn hook_script_returns_script_content() {
        let config = Config {
            projects: Some(HashMap::from([(
                "my-project".to_string(),
                ProjectConfig {
                    hooks: Some(HooksConfig {
                        new_after: Some("echo hello".to_string()),
                    }),
                },
            )])),
        };
        assert_eq!(
            hook_script(&config, "my-project", "new-after"),
            Some("echo hello")
        );
    }

    #[test]
    fn hook_script_returns_none_for_unknown_hook() {
        let config = Config {
            projects: Some(HashMap::from([(
                "my-project".to_string(),
                ProjectConfig {
                    hooks: Some(HooksConfig {
                        new_after: Some("echo hello".to_string()),
                    }),
                },
            )])),
        };
        assert!(hook_script(&config, "my-project", "unknown-hook").is_none());
    }

    #[test]
    fn deserialize_config_from_toml() {
        let toml_str = r#"
[projects.my-project.hooks]
new-after = """
#!/bin/bash
echo "hello"
"""
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let script = hook_script(&config, "my-project", "new-after").unwrap();
        assert!(script.contains("echo \"hello\""));
    }

    #[test]
    fn project_hook_script_returns_script() {
        let project = ProjectConfig {
            hooks: Some(HooksConfig {
                new_after: Some("echo project".to_string()),
            }),
        };
        assert_eq!(project_hook_script(&project, "new-after"), Some("echo project"));
    }

    #[test]
    fn project_hook_script_returns_none_without_hooks() {
        let project = ProjectConfig::default();
        assert!(project_hook_script(&project, "new-after").is_none());
    }

    #[test]
    fn deserialize_project_config_from_toml() {
        let toml_str = r#"
[hooks]
new-after = """
#!/bin/bash
echo "local"
"""
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        let script = project_hook_script(&config, "new-after").unwrap();
        assert!(script.contains("echo \"local\""));
    }
}
