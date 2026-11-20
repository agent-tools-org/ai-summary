//! Define CLI options and parse the default search command.
//! Exports Cli, Commands, and parse().

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ai-summary",
    version,
    about = "Web search & summarization with local LLM. Save tokens, search smarter.",
    long_about = "Search the web, fetch pages, or pipe text — summarize with a local LLM.\n\n\
        Examples:\n  \
        ai-summary what is Rust              Search + summarize\n  \
        ai-summary fetch <urls> -p <q>       Fetch URLs + summarize\n  \
        echo text | ai-summary sum <prompt>  Summarize stdin\n  \
        ai-summary github owner/repo -p <q>  Read GitHub code\n\n\
        Designed for Claude Code and AI coding agents to reduce token consumption."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    /// Search multiple angles, read and cross-check up to ten source pages
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
    #[arg(
        long,
        env = "AI_SUMMARY_API_KEY",
        hide_env_values = true,
        global = true
    )]
    pub api_key: Option<String>,
    #[arg(long, env = "AI_SUMMARY_MODEL", global = true)]
    pub model: Option<String>,
    /// Bypass summary cache
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Output structured JSON metadata
    #[arg(long, global = true)]
    pub metadata: bool,
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Search the web and summarize results
    #[command(visible_alias = "s")]
    Search {
        query: Vec<String>,
    },
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
    /// Report tools and summary route readiness without generating content
    Doctor {
        /// Check the configured OpenRouter key with a read-only request
        #[arg(long)]
        check_hosted: bool,
    },
    /// Clear the summary cache
    ResetCache,
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
    Github {
        /// Search query, or owner/repo[/path]
        #[arg(num_args = 1..)]
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
    #[command(about = "Configure LLM backend (OpenRouter API key, etc.)")]
    Setup {
        /// Set OpenRouter API key for direct API calls (no subprocess overhead)
        #[arg(long)]
        openrouter_key: Option<String>,
    },
}

pub fn parse() -> Cli {
    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if error.kind() == clap::error::ErrorKind::InvalidSubcommand => {
            let mut args = std::env::args_os();
            let executable = args.next().unwrap_or_else(|| "ai-summary".into());
            Cli::try_parse_from([executable, "search".into()].into_iter().chain(args))
                .unwrap_or_else(|_| error.exit())
        }
        Err(error) => error.exit(),
    }
}
