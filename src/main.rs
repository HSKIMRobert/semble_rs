use std::io::Read;
use std::process;

use clap::{Parser, Subcommand};

use semble::digest::{self, Format};
use semble::filter::smart_strip;
use semble::index::SembleIndex;
use semble::outline::extract_signature_near;
use semble::plan::{build_plan, print_plan};
use semble::stats::format_savings_report;
use semble::types::SearchResult;
use semble::utils::{format_results, is_git_url, resolve_chunk};

#[derive(Parser)]
#[command(name = "semble_rs", about = "Fast and Accurate Code Search for Agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search a codebase with keyword/symbol query
    Search {
        /// Keyword, symbol, or function name to search for
        query: String,
        /// Local path or git URL (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Number of results
        #[arg(short = 'k', long = "top-k", default_value = "10")]
        top_k: usize,
        /// Also index non-code text files (.md, .yaml, .json, etc.)
        #[arg(long)]
        include_text_files: bool,
        /// Output as JSON (for agent/tool integration)
        #[arg(long)]
        json: bool,
        /// Compact output: file paths, scores, and match lines only (minimal tokens)
        #[arg(long)]
        compact: bool,
        /// Strip comments from code chunks in JSON output to reduce tokens
        #[arg(long)]
        strip: bool,
        /// Outline output: one signature line per chunk (smallest token footprint)
        #[arg(long)]
        outline: bool,
        /// Group results by directory + cap match lines at 3 per chunk
        #[arg(long)]
        group: bool,
    },
    /// Find code similar to a specific location
    FindRelated {
        /// File path as shown in search results
        file_path: String,
        /// Line number (1-indexed)
        line: usize,
        /// Local path or git URL (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Number of results
        #[arg(short = 'k', long = "top-k", default_value = "10")]
        top_k: usize,
        /// Also index non-code text files
        #[arg(long)]
        include_text_files: bool,
        /// Output as JSON (for agent/tool integration)
        #[arg(long)]
        json: bool,
    },
    /// Show what a file depends on and what symbols it defines
    Deps {
        /// File path (relative to project root)
        file_path: String,
        /// Local path (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show all files affected if a file changes (transitive)
    Impact {
        /// File path (relative to project root)
        file_path: String,
        /// Local path (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Recommend a token-efficient exploration flow for a task
    Plan {
        /// Natural-language task or feature to investigate
        task: String,
        /// Local path or git URL (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Number of candidate chunks to use
        #[arg(short = 'k', long = "top-k", default_value = "8")]
        top_k: usize,
        /// Also index non-code text files (.md, .yaml, .json, etc.)
        #[arg(long)]
        include_text_files: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show token savings and usage stats
    Savings {
        /// Show usage breakdown by call type
        #[arg(long)]
        verbose: bool,
    },
    /// Compress build/test/install/CI output (cargo, pnpm, tsc, pytest, GitHub Actions)
    Digest {
        /// Input file. If omitted, reads from stdin.
        file: Option<String>,
        /// Force a specific format (auto-detects if omitted).
        /// Values: cargo, pnpm, tsc, pytest, ci.
        #[arg(long, default_value = "auto")]
        format: String,
        /// Print the detected format on stderr.
        #[arg(long)]
        show_format: bool,
    },
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Digest {
            file,
            format,
            show_format,
        } => {
            let text = match file {
                Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
                    eprintln!("Error reading {path}: {e}");
                    process::exit(1);
                }),
                None => {
                    let mut buf = String::new();
                    if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                        eprintln!("Error reading stdin: {e}");
                        process::exit(1);
                    }
                    buf
                }
            };
            let fmt = if format == "auto" {
                digest::detect(&text)
            } else {
                Format::parse(&format).unwrap_or_else(|| {
                    eprintln!("Unknown --format value: {format}. Valid: cargo, pnpm, tsc, pytest, ci, auto.");
                    process::exit(1);
                })
            };
            if show_format {
                eprintln!("[digest] format={}", fmt.as_str());
            }
            let out = digest::digest(&text, fmt);
            println!("{out}");
        }
        Commands::Savings { verbose } => {
            print!("{}", format_savings_report(verbose));
        }
        Commands::Deps {
            file_path,
            path,
            json,
        } => {
            let index = build_index(&path, false);
            let graph = index.graph();

            if json {
                match graph.deps(&file_path) {
                    Some(node) => {
                        println!(
                            "{}",
                            serde_json::to_string(node).unwrap_or_else(|_| "{}".to_string())
                        );
                    }
                    None => {
                        println!("{{}}");
                    }
                }
            } else {
                match graph.deps(&file_path) {
                    Some(node) => {
                        println!("File: {file_path}");
                        println!();
                        if !node.symbols.is_empty() {
                            println!("Symbols ({}):", node.symbols.len());
                            for sym in &node.symbols {
                                println!("  {} {} (line {})", sym.kind, sym.name, sym.line);
                            }
                            println!();
                        }
                        if !node.depends_on.is_empty() {
                            println!("Depends on ({}):", node.depends_on.len());
                            for dep in &node.depends_on {
                                println!("  {dep}");
                            }
                            println!();
                        }
                        let dependents = graph.dependents(&file_path);
                        if !dependents.is_empty() {
                            println!("Used by ({}):", dependents.len());
                            for dep in &dependents {
                                println!("  {dep}");
                            }
                        }
                        if node.symbols.is_empty()
                            && node.depends_on.is_empty()
                            && dependents.is_empty()
                        {
                            println!("No dependencies or symbols found.");
                        }
                    }
                    None => {
                        eprintln!("File not found in graph: {file_path}");
                        process::exit(1);
                    }
                }
            }
        }
        Commands::Impact {
            file_path,
            path,
            json,
        } => {
            let index = build_index(&path, false);
            let graph = index.graph();
            let affected = graph.impact(&file_path);

            if json {
                println!(
                    "{}",
                    serde_json::to_string(&affected).unwrap_or_else(|_| "[]".to_string())
                );
            } else if affected.is_empty() {
                println!("No files affected by changes to {file_path}.");
            } else {
                println!("Impact of {file_path} ({} files affected):", affected.len());
                println!();
                for f in &affected {
                    println!("  {f}");
                }
            }
        }
        Commands::Plan {
            task,
            path,
            top_k,
            include_text_files,
            json,
        } => {
            let index = build_index(&path, include_text_files);
            let results = index.search(task.as_str(), top_k, None, None, None);
            let report = build_plan(&task, &path, top_k, &results);

            if json {
                println!(
                    "{}",
                    serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_string())
                );
            } else {
                print_plan(&report);
            }
        }
        Commands::Search {
            query,
            path,
            top_k,
            include_text_files,
            json,
            compact,
            strip,
            outline,
            group,
        } => {
            let index = build_index(&path, include_text_files);

            let results = index.search(query.as_str(), top_k, None, None, None);
            if outline {
                print_outline(&results);
            } else if group {
                print_grouped(&results);
            } else if compact {
                print_compact(&results);
            } else if json && strip {
                print_json_stripped(&results);
            } else if json {
                print_json(&results);
            } else if results.is_empty() {
                println!("No results found.");
            } else {
                println!(
                    "{}",
                    format_results(&format!("Search results for: {query:?}"), &results)
                );
            }
        }
        Commands::FindRelated {
            file_path,
            line,
            path,
            top_k,
            include_text_files,
            json,
        } => {
            let index = build_index(&path, include_text_files);

            let chunk = match resolve_chunk(index.chunks(), &file_path, line) {
                Some(c) => c.clone(),
                None => {
                    eprintln!("No chunk found at {file_path}:{line}.");
                    process::exit(1);
                }
            };

            let results = index.find_related(&chunk, top_k);
            if json {
                print_json(&results);
            } else if results.is_empty() {
                println!("No related chunks found for {file_path}:{line}.");
            } else {
                println!(
                    "{}",
                    format_results(&format!("Chunks related to {file_path}:{line}"), &results)
                );
            }
        }
    }
}

fn print_compact(results: &[SearchResult]) {
    for r in results {
        println!(
            "{:.4}\t{}:{}-{}",
            r.score, r.chunk.file_path, r.chunk.start_line, r.chunk.end_line
        );
        for ml in &r.match_lines {
            println!("  L{}:\t{}", ml.line, truncate_line(&ml.content, 120));
        }
    }
}

fn print_outline(results: &[SearchResult]) {
    for r in results {
        let match_nums: Vec<usize> = r.match_lines.iter().map(|m| m.line).collect();
        let sig = extract_signature_near(&r.chunk.content, r.chunk.start_line, &match_nums)
            .unwrap_or_else(|| format!("(lines {}-{})", r.chunk.start_line, r.chunk.end_line));
        let match_suffix = if r.match_lines.is_empty() {
            String::new()
        } else {
            format!(" [{}m]", r.match_lines.len())
        };
        println!(
            "{:.4} {}:{}-{}{}\n  {}",
            r.score, r.chunk.file_path, r.chunk.start_line, r.chunk.end_line, match_suffix, sig
        );
    }
}

fn print_grouped(results: &[SearchResult]) {
    use std::collections::BTreeMap;
    let mut by_dir: BTreeMap<String, (f64, Vec<&SearchResult>)> = BTreeMap::new();
    for r in results {
        let dir = std::path::Path::new(&r.chunk.file_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        let entry = by_dir.entry(dir).or_insert((f64::NEG_INFINITY, Vec::new()));
        if r.score > entry.0 {
            entry.0 = r.score;
        }
        entry.1.push(r);
    }
    let mut dirs: Vec<(&String, &(f64, Vec<&SearchResult>))> = by_dir.iter().collect();
    dirs.sort_by(|a, b| {
        b.1 .0
            .partial_cmp(&a.1 .0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    const MAX_MATCH_LINES: usize = 3;
    for (dir, (_, group)) in dirs {
        let has_dir = !dir.is_empty();
        if has_dir {
            println!("{dir}/");
        }
        for r in group {
            let fname = std::path::Path::new(&r.chunk.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(r.chunk.file_path.as_str());
            let indent = if has_dir { "  " } else { "" };
            println!(
                "{indent}{:.4} {fname}:{}-{}",
                r.score, r.chunk.start_line, r.chunk.end_line
            );
            let total = r.match_lines.len();
            for ml in r.match_lines.iter().take(MAX_MATCH_LINES) {
                println!(
                    "{indent}  L{}: {}",
                    ml.line,
                    truncate_line(&ml.content, 100)
                );
            }
            if total > MAX_MATCH_LINES {
                println!("{indent}  ... (+{})", total - MAX_MATCH_LINES);
            }
        }
    }
}

fn truncate_line(line: &str, max_len: usize) -> String {
    let trimmed = line.trim();
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }
    let s: String = trimmed.chars().take(max_len - 3).collect();
    format!("{s}...")
}

fn print_json_stripped(results: &[SearchResult]) {
    let stripped: Vec<SearchResult> = results
        .iter()
        .map(|r| {
            let lang = r.chunk.language.as_deref();
            SearchResult {
                chunk: semble::types::Chunk::new(
                    smart_strip(&r.chunk.content, lang),
                    r.chunk.file_path.clone(),
                    r.chunk.start_line,
                    r.chunk.end_line,
                    r.chunk.language.clone(),
                ),
                score: r.score,
                match_lines: r.match_lines.clone(),
            }
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&stripped).unwrap_or_else(|_| "[]".to_string())
    );
}

fn print_json(results: &[SearchResult]) {
    println!(
        "{}",
        serde_json::to_string(results).unwrap_or_else(|_| "[]".to_string())
    );
}

fn build_index(path: &str, include_text_files: bool) -> SembleIndex {
    let result = if is_git_url(path) {
        SembleIndex::from_git(path, None, None, None, None, include_text_files)
    } else {
        SembleIndex::from_path(path, None, None, None, include_text_files)
    };

    match result {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: {e:?}");
            process::exit(1);
        }
    }
}
