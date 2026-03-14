// CLI entrypoint and command dispatch for ai-summary.
// Exports: Cli, Commands, main().
// Deps: crate modules for config, commands, crawl, compress, stats; clap; reqwest.

mod bench;
mod commands;
mod compress;
mod config;
mod crawl;
mod fetch;
mod fetch_utils;
mod llm;
mod search;
mod stats;
mod stats_history;
mod types;
mod wrap;
mod init;

use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use std::fs;
use std::io::Read;

use crate::bench::run_bench;
use crate::commands::{run_fetch, run_github, run_repo, run_search, run_summarize};
use crate::compress::run_compress;
use crate::config::{print_config, resolve_config};
use crate::crawl::run_crawl;
use crate::stats::{print_stats, print_stats_json, stats_path};
use crate::wrap::run_wrap;
use crate::init::run_init;

#[derive(Parser)]
#[command(
    name = "ai-summary",
    version,
    about = "Web search & summarization with local LLM. Save tokens, search smarter.",
    long_about = "Search the web, fetch pages, or pipe text — summarize with a local LLM.\n\n\
        Three modes:\n  \
        1. ai-summary <query>           Search + fetch + summarize\n  \
        2. ai-summary fetch <urls>      Fetch URLs + summarize\n  \
        3. echo text | ai-summary sum   Summarize stdin text\n\n\
        Designed for Claude Code and AI coding agents to reduce token consumption."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(long, global = true)]
    pub deep: bool,
    #[arg(long, global = true)]
    pub raw: bool,
    #[arg(long, global = true)]
    pub cf: bool,
    #[arg(long, global = true)]
    pub browser: bool,
    #[arg(long, env = "AI_SUMMARY_API_URL", global = true)]
    pub api_url: Option<String>,
    #[arg(long, env = "AI_SUMMARY_API_KEY", global = true)]
    pub api_key: Option<String>,
    #[arg(long, env = "AI_SUMMARY_MODEL", global = true)]
    pub model: Option<String>,
    #[arg(long, global = true)]
    pub json: bool,
    pub query: Vec<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(visible_alias = "sum")]
    Summarize {
        prompt: Vec<String>,
    },
    Fetch {
        urls: Vec<String>,
        #[arg(short, long)]
        prompt: Option<String>,
    },
    Compress {
        #[arg(short, long, default_value = "4000")]
        max_chars: usize,
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Run a command, compress large passing test output, preserve exit code.
    /// Used by PreToolUse hook to rewrite test commands.
    Wrap {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    Stats,
    ResetStats,
    Config,
    /// Benchmark fetch + summarization on a set of test URLs.
    Bench,
    Crawl {
        url: String,
        #[arg(short, long)]
        prompt: Option<String>,
        #[arg(short, long, default_value = "10")]
        limit: u32,
        #[arg(short, long, default_value = "2")]
        depth: u32,
    },
    /// Search GitHub code or read files from repos via gh CLI.
    #[command(visible_alias = "gh")]
    Github {
        /// Search query, or owner/repo[/path]
        args: Vec<String>,
        /// Restrict search to a specific repo (owner/repo)
        #[arg(short, long)]
        repo: Option<String>,
        /// Filter by programming language
        #[arg(short, long)]
        language: Option<String>,
        /// Question to answer about the code
        #[arg(short, long)]
        prompt: Option<String>,
    },
    /// Pack a remote GitHub repo with repomix and summarize it.
    Repo {
        /// GitHub URL or owner/repo shorthand
        repo: String,
        #[arg(short, long)]
        prompt: Option<String>,
        /// Glob patterns for repomix --include (e.g. "src/**/*.rs,*.toml")
        #[arg(short = 'I', long)]
        include: Option<String>,
    },
    #[command(about = "Install Claude Code integration (prompt + hook)")]
    Init {
        #[arg(long)]
        uninstall: bool,
        /// Also install repomix globally (npm install -g repomix)
        #[arg(long)]
        with_repomix: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let cfg = resolve_config(&cli);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("HTTP client");

    match &cli.command {
        Some(Commands::Stats) => {
            if cli.json {
                print_stats_json();
            } else {
                print_stats();
            }
            return;
        }
        Some(Commands::ResetStats) => {
            let _ = fs::remove_file(stats_path());
            println!("Statistics reset.");
            return;
        }
        Some(Commands::Config) => {
            print_config();
            return;
        }
        Some(Commands::Init { uninstall, with_repomix }) => {
            run_init(*uninstall, *with_repomix);
            return;
        }
        Some(Commands::Wrap { command }) => {
            run_wrap(command);
            // run_wrap calls process::exit — never returns
        }
        Some(Commands::Compress { max_chars, source }) => {
            run_compress(*max_chars, source.as_deref());
            return;
        }
        Some(Commands::Summarize { prompt }) => {
            run_summarize(&cli, &cfg, &client, &prompt.join(" "));
            return;
        }
        Some(Commands::Fetch { urls, prompt }) => {
            let urls = if urls.is_empty() && !atty::is(atty::Stream::Stdin) {
                let mut input = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut input) {
                    eprintln!("[ai-summary] Failed to read stdin: {e}");
                    std::process::exit(1);
                }
                input
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            } else {
                urls.clone()
            };
            if urls.is_empty() {
                eprintln!("Error: No URLs provided.");
                std::process::exit(1);
            }
            run_fetch(&cli, &cfg, &client, &urls, prompt);
            return;
        }
        Some(Commands::Bench) => {
            run_bench(&cfg, &client);
            return;
        }
        Some(Commands::Crawl {
            url,
            prompt,
            limit,
            depth,
        }) => {
            run_crawl(&cli, &cfg, &client, url, prompt, *limit, *depth);
            return;
        }
        Some(Commands::Github {
            args,
            repo,
            language,
            prompt,
        }) => {
            run_github(&cli, &cfg, &client, args, repo, language, prompt);
            return;
        }
        Some(Commands::Repo {
            repo,
            prompt,
            include,
        }) => {
            run_repo(&cli, &cfg, &client, repo, prompt, include.as_deref());
            return;
        }
        None => {}
    }

    let query = cli.query.join(" ");
    if query.is_empty() {
        eprintln!("ai-summary - Web search & summarization with local LLM\n");
        eprintln!("Usage:");
        eprintln!("  ai-summary <query>                      Search + summarize");
        eprintln!("  ai-summary fetch <url1> <url2> -p <q>   Fetch URLs + summarize");
        eprintln!("  echo text | ai-summary sum <prompt>      Summarize stdin");
        eprintln!(
            "  ai-summary crawl <url> -p <q>            Crawl website (CF Browser Rendering)"
        );
        eprintln!("  ai-summary github <query> [-r repo]      Search GitHub code");
        eprintln!("  ai-summary github owner/repo [path]      Read file from repo");
        eprintln!("  ai-summary stats                        Show token savings");
        eprintln!("  ai-summary config                       Show/create config");
        eprintln!("\nFlags: --deep --raw --cf --browser --api-url --api-key --model");
        std::process::exit(1);
    }

    run_search(&cli, &cfg, &client, &query);
}
