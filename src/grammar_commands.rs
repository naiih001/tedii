use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::config::Config;

pub fn fetch_grammars(config: &Config, runtime: &Path) -> Result<()> {
    let sources_dir = runtime.join("sources");
    std::fs::create_dir_all(&sources_dir)?;

    for grammar in &config.grammars {
        let dest = sources_dir.join(&grammar.name);
        println!("Fetching grammar '{}' from {} ...", grammar.name, grammar.source);

        let url = tarball_url(&grammar.source)?;

        let response = reqwest::blocking::Client::builder()
            .user_agent("tedii-editor")
            .build()?
            .get(&url)
            .send()
            .with_context(|| format!("Failed to download {}", url))?;

        if !response.status().is_success() {
            bail!(
                "Download failed for '{}': HTTP {}",
                grammar.name,
                response.status()
            );
        }

        let bytes = response.bytes()?;

        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        std::fs::create_dir_all(&dest)?;

        let mut child = Command::new("tar")
            .args(["-xzf", "-", "-C"])
            .arg(&dest)
            .arg("--strip-components=1")
            .stdin(Stdio::piped())
            .spawn()
            .context("Failed to run tar. Is tar installed?")?;

        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().context("Failed to open tar stdin")?;
            stdin.write_all(&bytes)?;
        }

        let status = child.wait()?;
        if !status.success() {
            bail!("tar extraction failed for '{}'", grammar.name);
        }

        println!("  ✓ Downloaded and extracted to {}", dest.display());
    }

    Ok(())
}

pub fn build_grammars(config: &Config, runtime: &Path) -> Result<()> {
    let sources_dir = runtime.join("sources");
    let grammars_dir = runtime.join("grammars");
    let queries_dir = runtime.join("queries");

    std::fs::create_dir_all(&grammars_dir)?;

    for grammar in &config.grammars {
        let source_dir = sources_dir.join(&grammar.name);
        if !source_dir.exists() {
            eprintln!(
                "Warning: source for '{}' not found at {}. Run 'tedii --grammar fetch' first.",
                grammar.name,
                source_dir.display()
            );
            continue;
        }

        let src_dir = if source_dir.join("src").join("parser.c").exists() {
            source_dir.join("src")
        } else if source_dir.join(&grammar.name).join("src").join("parser.c").exists() {
            source_dir.join(&grammar.name).join("src")
        } else {
            eprintln!(
                "Warning: parser.c not found for '{}'. Skipping.",
                grammar.name
            );
            continue;
        };

        let parser_c = src_dir.join("parser.c");
        let scanner_c = src_dir.join("scanner.c");
        let has_scanner = scanner_c.exists();

        let output = grammars_dir.join(format!("{}.so", grammar.name));
        println!("Building grammar '{}' ...", grammar.name);

        let mut cmd = Command::new("cc");
        cmd.args(["-shared", "-fPIC", "-o"])
            .arg(&output)
            .arg(&parser_c)
            .arg("-I")
            .arg(&src_dir);

        if has_scanner {
            cmd.arg(&scanner_c);
        }

        let status = cmd
            .current_dir(&source_dir)
            .status()
            .context("Failed to run cc. Is a C compiler installed?")?;

        if !status.success() {
            eprintln!("  ✗ Compilation failed for '{}'", grammar.name);
            continue;
        }

        println!("  ✓ Compiled to {}", output.display());

        // Copy queries
        let source_queries = source_dir.join("queries");
        if source_queries.exists() {
            let dest_queries = queries_dir.join(&grammar.name);
            std::fs::create_dir_all(&dest_queries)?;

            for entry in std::fs::read_dir(&source_queries)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let src_path = entry.path();
                if src_path.is_file() {
                    let dst_path = dest_queries.join(&file_name);
                    std::fs::copy(&src_path, &dst_path)?;
                    println!("  ✓ Copied queries/{}", file_name.to_string_lossy());
                }
            }
        }
    }

    Ok(())
}

pub fn update_grammars(config: &Config, runtime: &Path) -> Result<()> {
    fetch_grammars(config, runtime)?;
    build_grammars(config, runtime)?;
    println!("Done.");
    Ok(())
}

fn tarball_url(source: &str) -> Result<String> {
    // Support formats:
    //   https://github.com/owner/repo
    //   owner/repo
    let trimmed = source.trim().trim_end_matches(".git");

    let parts: Vec<&str> = if trimmed.starts_with("https://github.com/") {
        let path = trimmed.trim_start_matches("https://github.com/");
        path.split('/').collect()
    } else if trimmed.starts_with("http://github.com/") {
        let path = trimmed.trim_start_matches("http://github.com/");
        path.split('/').collect()
    } else {
        trimmed.split('/').collect()
    };

    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "Invalid grammar source '{}'. Expected format: owner/repo or https://github.com/owner/repo",
            source
        );
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");

    Ok(format!(
        "https://api.github.com/repos/{}/{}/tarball",
        owner, repo
    ))
}

pub fn find_or_create_runtime() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?;
    let path = config_dir.join("tedii").join("runtime");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn create_default_config() -> Result<()> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join("tedii");

    let config_path = config_dir.join("languages.toml");
    if config_path.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&config_dir)?;
    let default = r#"# tedii language configuration

[[language]]
name = "rust"
file-types = ["rs"]
grammar = "rust"

[[language]]
name = "python"
file-types = ["py"]
grammar = "python"

[[language]]
name = "javascript"
file-types = ["js"]
grammar = "javascript"

[[language]]
name = "typescript"
file-types = ["ts"]
grammar = "typescript"

[[language]]
name = "go"
file-types = ["go"]
grammar = "go"

[[language]]
name = "c"
file-types = ["c", "h"]
grammar = "c"

[[language]]
name = "cpp"
file-types = ["cpp", "hpp", "cc", "cxx"]
grammar = "cpp"

[[grammar]]
name = "rust"
source = "https://github.com/tree-sitter/tree-sitter-rust"

[[grammar]]
name = "python"
source = "https://github.com/tree-sitter/tree-sitter-python"

[[grammar]]
name = "javascript"
source = "https://github.com/tree-sitter/tree-sitter-javascript"

[[grammar]]
name = "typescript"
source = "https://github.com/tree-sitter/tree-sitter-typescript"

[[grammar]]
name = "go"
source = "https://github.com/tree-sitter/tree-sitter-go"

[[grammar]]
name = "c"
source = "https://github.com/tree-sitter/tree-sitter-c"

[[grammar]]
name = "cpp"
source = "https://github.com/tree-sitter/tree-sitter-cpp"
"#;
    std::fs::write(&config_path, default)?;
    println!("Created default config at {}", config_path.display());
    println!("Run 'tedii --grammar update' to fetch and build grammars.");
    Ok(())
}
