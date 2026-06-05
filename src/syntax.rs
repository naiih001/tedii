use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator, TextProvider};

use crate::theme::Theme;

struct LoadedGrammar {
    language: Language,
    _library: Library,
}

pub struct GrammarLoader {
    grammars: HashMap<String, LoadedGrammar>,
}

impl GrammarLoader {
    pub fn new() -> Self {
        Self {
            grammars: HashMap::new(),
        }
    }

    pub fn load_grammar(&mut self, name: &str, path: &Path) -> Result<()> {
        let lib = unsafe {
            Library::new(path)
                .with_context(|| format!("Failed to load grammar library: {}", path.display()))?
        };

        let symbol_name = format!("tree_sitter_{}", name.replace('-', "_"));
        let func: Symbol<unsafe extern "C" fn() -> *const tree_sitter::ffi::TSLanguage> =
            unsafe {
                lib.get(symbol_name.as_bytes()).with_context(|| {
                    format!("Symbol {} not found in {}", symbol_name, path.display())
                })?
            };

        let lang_ptr = unsafe { func() };
        let language = unsafe { Language::from_raw(lang_ptr) };

        self.grammars.insert(
            name.to_string(),
            LoadedGrammar {
                language,
                _library: lib,
            },
        );

        Ok(())
    }

    pub fn get_language(&self, name: &str) -> Option<Language> {
        self.grammars.get(name).map(|g| g.language.clone())
    }

    pub fn is_loaded(&self, name: &str) -> bool {
        self.grammars.contains_key(name)
    }
}

struct SliceProvider<'a>(&'a [u8]);

impl<'a> TextProvider<&'a [u8]> for SliceProvider<'a> {
    type I = std::iter::Once<&'a [u8]>;

    fn text(&mut self, node: Node) -> Self::I {
        std::iter::once(&self.0[node.start_byte()..node.end_byte()])
    }
}

pub struct SyntaxHighlighter {
    loader: GrammarLoader,
    parser: Parser,
    configs: HashMap<String, (Query, Vec<String>)>,
    theme: Theme,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            loader: GrammarLoader::new(),
            parser: Parser::new(),
            configs: HashMap::new(),
            theme: Theme::default_theme(),
        }
    }

    pub fn load_language(
        &mut self,
        name: &str,
        grammar_path: &Path,
        query_source: &str,
    ) -> Result<()> {
        if !self.loader.is_loaded(name) {
            self.loader.load_grammar(name, grammar_path)?;
        }

        let language = self
            .loader
            .get_language(name)
            .context("Grammar not loaded after loading")?;

        let query = Query::new(&language, query_source)
            .with_context(|| format!("Failed to compile query for language '{}'", name))?;

        let capture_names: Vec<String> = query.capture_names().iter().map(|s| s.to_string()).collect();

        self.configs.insert(name.to_string(), (query, capture_names));

        Ok(())
    }

    pub fn load_language_for_path(&mut self, file_path: &str) -> Option<String> {
        let lang = self.language_for_file(file_path)?;
        let runtime = find_runtime_dir()?;

        let grammar_path = runtime.join("grammars").join(format!("{}.so", lang));
        let query_dir = runtime.join("queries").join(&lang);

        if !grammar_path.exists() || !query_dir.join("highlights.scm").exists() {
            return Some(lang);
        }

        if self.configs.contains_key(&lang) {
            return Some(lang);
        }

        let highlights_path = query_dir.join("highlights.scm");
        let query_source = if lang == "typescript" {
            let js_path = runtime.join("queries").join("javascript").join("highlights.scm");
            let mut combined = String::new();
            if let Ok(js) = std::fs::read_to_string(&js_path) {
                combined.push_str(&js);
                combined.push('\n');
            }
            if let Ok(ts) = std::fs::read_to_string(&highlights_path) {
                combined.push_str(&ts);
            }
            combined
        } else {
            std::fs::read_to_string(&highlights_path).ok().unwrap_or_default()
        };

        self.load_language(&lang, &grammar_path, &query_source).ok();
        Some(lang)
    }

    pub fn language_for_file(&self, filename: &str) -> Option<String> {
        let ext = Path::new(filename).extension()?.to_str()?;
        match ext {
            "rs" => Some("rust".to_string()),
            "py" => Some("python".to_string()),
            "js" => Some("javascript".to_string()),
            "ts" => Some("typescript".to_string()),
            "go" => Some("go".to_string()),
            "c" | "h" => Some("c".to_string()),
            "cpp" | "hpp" | "cc" | "cxx" => Some("cpp".to_string()),
            "toml" => Some("toml".to_string()),
            "json" => Some("json".to_string()),
            "md" => Some("markdown".to_string()),
            "sh" | "bash" | "zsh" => Some("bash".to_string()),
            "lua" => Some("lua".to_string()),
            "zig" => Some("zig".to_string()),
            "java" => Some("java".to_string()),
            "rb" => Some("ruby".to_string()),
            _ => None,
        }
    }

    pub fn highlight(
        &mut self,
        source: &str,
        lang: &str,
    ) -> Vec<(usize, usize, ratatui::style::Style)> {
        let (query, capture_names) = match self.configs.get(lang) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let language = match self.loader.get_language(lang) {
            Some(l) => l,
            None => return Vec::new(),
        };

        if self.parser.set_language(&language).is_err() {
            return Vec::new();
        }

        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root_node = tree.root_node();
        let mut cursor = QueryCursor::new();
        let provider = SliceProvider(source.as_bytes());
        let mut caps = cursor.captures(query, root_node, provider);

        let mut highlights: Vec<(usize, usize, ratatui::style::Style)> = Vec::new();

        while let Some((match_, capture_idx)) = caps.next() {
            let capture = &match_.captures[*capture_idx];
            if let Some(name) = capture_names.get(capture.index as usize) {
                let style = self.theme.style_for_capture(name);
                let node = capture.node;
                let start = node.start_byte();
                let end = node.end_byte();
                highlights.push((start, end, style));
            }
        }

        // Sort by start byte ascending, end byte descending.
        // Wider ranges first, so narrower (innermost) ranges come later and overwrite.
        highlights.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1))
        });

        highlights
    }
}

pub fn find_runtime_dir() -> Option<PathBuf> {
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("tedii").join("runtime");
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(path) = std::env::var("tedii_RUNTIME") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let path = exe_dir.join("runtime");
            if path.exists() {
                return Some(path);
            }
        }
    }

    let path = PathBuf::from("runtime");
    if path.exists() {
        return Some(path);
    }

    None
}
