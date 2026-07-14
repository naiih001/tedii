use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct LanguageConfig {
    pub name: String,
    #[serde(rename = "file-types")]
    pub file_types: Vec<String>,
    pub grammar: String,
    pub highlights: Option<PathBuf>,
    #[serde(default)]
    pub lsp: Option<LspServerConfig>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct LspServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct GrammarDef {
    pub name: String,
    pub source: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "language", default)]
    pub languages: Vec<LanguageConfig>,
    #[serde(rename = "grammar", default)]
    pub grammars: Vec<GrammarDef>,
}

#[allow(dead_code)]
impl Config {
    pub fn grammar_path(&self, name: &str) -> Option<PathBuf> {
        let _grammar = self.grammars.iter().find(|g| g.name == name)?;
        let config_dir = dirs::config_dir()?;
        Some(
            config_dir
                .join("tedii")
                .join("grammars")
                .join(format!("{}.so", name)),
        )
    }

    pub fn language_for_file(&self, filename: &str) -> Option<&LanguageConfig> {
        let ext = std::path::Path::new(filename).extension()?.to_str()?;
        self.languages.iter().find(|language| {
            language
                .file_types
                .iter()
                .any(|file_type| file_type == ext)
        })
    }
}

pub fn load_config() -> Result<Config> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join("tedii");

    let config_path = config_dir.join("languages.toml");

    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Configuration file not found at {:?}",
            config_path
        ));
    }

    let content = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&content)?;

    Ok(config)
}

// --- Theme config from config.toml ---

#[derive(Debug, Deserialize)]
pub struct ColorDef {
    pub fg: String,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub modifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ThemeConfig {
    #[serde(default)]
    pub syntax: HashMap<String, ColorDef>,
    #[serde(default)]
    pub ui: HashMap<String, ColorDef>,
}

#[derive(Debug, Deserialize)]
pub struct KeybindingsConfig {
    #[serde(default = "default_leader_keys")]
    pub leader_keys: bool,
}

fn default_leader_keys() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct ConfigToml {
    #[serde(default)]
    theme: Option<ThemeConfig>,
    #[serde(default)]
    keybindings: Option<KeybindingsConfig>,
}

pub fn load_theme_config() -> Option<ThemeConfig> {
    let config_dir = dirs::config_dir()?;
    let config_path = config_dir.join("tedii").join("config.toml");
    if !config_path.exists() {
        return None;
    }
    let content = fs::read_to_string(config_path).ok()?;
    let parsed: ConfigToml = toml::from_str(&content).ok()?;
    parsed.theme
}

pub fn load_keybindings_config() -> Option<KeybindingsConfig> {
    let config_dir = dirs::config_dir()?;
    let config_path = config_dir.join("tedii").join("config.toml");
    if !config_path.exists() {
        return None;
    }
    let content = fs::read_to_string(config_path).ok()?;
    let parsed: ConfigToml = toml::from_str(&content).ok()?;
    parsed.keybindings
}
