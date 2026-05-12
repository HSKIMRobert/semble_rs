use std::process;

use clap::{Parser, Subcommand};

use semble::index::SembleIndex;
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
    /// Show token savings and usage stats
    Savings {
        /// Show usage breakdown by call type
        #[arg(long)]
        verbose: bool,
    },
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
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
                        println!("{}", serde_json::to_string(node).unwrap_or_else(|_| "{}".to_string()));
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
                        if node.symbols.is_empty() && node.depends_on.is_empty() && dependents.is_empty() {
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
                println!("{}", serde_json::to_string(&affected).unwrap_or_else(|_| "[]".to_string()));
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
        Commands::Search {
            query,
            path,
            top_k,
            include_text_files,
            json,
        } => {
            let index = build_index(&path, include_text_files);

            let results = index.search(query.as_str(), top_k, None, None, None);
            if json {
                print_json(&results);
            } else if results.is_empty() {
                println!("No results found.");
            } else {
                println!(
                    "{}",
                    format_results(
                        &format!("Search results for: {query:?}"),
                        &results
                    )
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
                    format_results(
                        &format!("Chunks related to {file_path}:{line}"),
                        &results
                    )
                );
            }
        }
    }
}

fn print_json(results: &[SearchResult]) {
    println!("{}", serde_json::to_string(results).unwrap_or_else(|_| "[]".to_string()));
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
