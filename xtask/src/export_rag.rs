use ignore::WalkBuilder;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

const MAX_LINES_PER_FILE: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Category {
    Logic,
    Config,
    Content,
    Misc,
    Excluded,
}

impl Category {
    fn prefix(&self) -> &'static str {
        match self {
            Category::Logic => "rag_logic",
            Category::Config => "rag_config",
            Category::Content => "rag_content",
            Category::Misc => "rag_misc",
            Category::Excluded => "rag_excluded",
        }
    }
}

pub fn export_rag(dirty_only: bool) {
    println!("Exporting RAG friendly archives...");
    if dirty_only {
        println!("Mode: --dirty-only (filtering to git-dirty files only)");
    }

    let out_dir = Path::new("rag_export");
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).expect("Failed to clear existing rag_export directory");
    }
    fs::create_dir_all(out_dir).expect("Failed to create rag_export directory");

    // ── Optional dirty-file filter ────────────────────────────────────────
    let dirty_filter: Option<HashSet<std::path::PathBuf>> = if dirty_only {
        use rustodian_core::traits::GitInspector;
        let inspector = rustodian_git::Git2Inspector;
        match inspector.get_dirty_files(Path::new(".")) {
            Ok(files) => {
                let set: HashSet<_> = files
                    .into_iter()
                    .map(|f| {
                        // Canonicalize for reliable comparison
                        f.canonicalize().unwrap_or(f)
                    })
                    .collect();
                println!("  Found {} dirty file(s) to export.", set.len());
                Some(set)
            }
            Err(e) => {
                eprintln!("Warning: could not query git status: {e}. Exporting all files.");
                None
            }
        }
    } else {
        None
    };

    let mut walker = WalkBuilder::new(".");
    walker.hidden(false); // We might want to see some hidden files like .gitignore, .github/
    walker.filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        if name == ".git" || name == "target" || name == "rag_export" {
            return false;
        }
        true
    });

    struct CategoryWriter {
        category: Category,
        file_index: usize,
        current_lines: usize,
        file: Option<File>,
    }

    impl CategoryWriter {
        fn new(category: Category) -> Self {
            Self {
                category,
                file_index: 1,
                current_lines: 0,
                file: None,
            }
        }

        fn get_file(&mut self) -> io::Result<&mut File> {
            if self.file.is_none() || self.current_lines >= MAX_LINES_PER_FILE {
                if self.file.is_some() {
                    self.file_index += 1;
                }
                let filename = format!("{}_{}.md", self.category.prefix(), self.file_index);
                let path = Path::new("rag_export").join(filename);
                let mut f = File::create(path)?;
                writeln!(
                    f,
                    "# RAG Export - {:?} (Part {})\n",
                    self.category, self.file_index
                )?;
                self.file = Some(f);
                self.current_lines = 2; // header
            }
            Ok(self.file.as_mut().unwrap())
        }

        fn write_entry(&mut self, path: &str, content: &str) -> io::Result<()> {
            let lines_in_content = content.lines().count() + 6; // +6 for markdown formatting
            let f = self.get_file()?;
            writeln!(f, "### Path: {}", path)?;
            writeln!(f, "```")?;
            writeln!(f, "{}", content)?;
            writeln!(f, "```\n")?;
            self.current_lines += lines_in_content;
            Ok(())
        }

        fn write_excluded(&mut self, path: &str, reason: &str) -> io::Result<()> {
            let f = self.get_file()?;
            writeln!(f, "- **Excluded:** `{}` (Reason: {})", path, reason)?;
            self.current_lines += 1;
            Ok(())
        }
    }

    let mut writers = HashMap::new();
    for &cat in &[
        Category::Logic,
        Category::Config,
        Category::Content,
        Category::Misc,
        Category::Excluded,
    ] {
        writers.insert(cat, CategoryWriter::new(cat));
    }

    for result in walker.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        // ── Dirty-only filter gate ────────────────────────────────────────
        if let Some(ref filter) = dirty_filter {
            let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if !filter.contains(&canon) {
                continue;
            }
        }

        // Skip some common generated or unhelpful stuff
        if path_str.contains("Cargo.lock") {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let category = match ext {
            "rs" | "py" | "js" | "ts" | "go" | "sh" | "c" | "cpp" | "h" => Category::Logic,
            "toml" | "json" | "yml" | "yaml" | "ini" => Category::Config,
            "md" | "txt" | "csv" => Category::Content,
            _ => {
                if file_name == "justfile"
                    || file_name == "Dockerfile"
                    || file_name == ".gitignore"
                    || file_name == ".editorconfig"
                {
                    Category::Config
                } else if file_name == "LICENSE-APACHE" || file_name == "LICENSE-MIT" {
                    Category::Content
                } else {
                    Category::Misc
                }
            }
        };

        // Try to read content
        match fs::read_to_string(path) {
            Ok(content) => {
                let writer = writers.get_mut(&category).unwrap();
                writer
                    .write_entry(&path_str, &content)
                    .expect("Failed to write entry");
            }
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                let writer = writers.get_mut(&Category::Excluded).unwrap();
                writer
                    .write_excluded(&path_str, "Binary file / Invalid UTF-8")
                    .expect("Failed to write excluded");
            }
            Err(e) => {
                let writer = writers.get_mut(&Category::Excluded).unwrap();
                writer
                    .write_excluded(&path_str, &format!("Read error: {}", e))
                    .expect("Failed to write excluded");
            }
        }
    }

    println!("✅ RAG archives generated in rag_export/ directory.");
}
