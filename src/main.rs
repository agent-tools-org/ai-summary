// CLI entrypoint and command dispatch for ai-summary.
// Exports: Cli, Commands, main().
// Deps: crate modules for config, commands, crawl, compress, stats; clap; reqwest.

mod bench;
mod cache;
mod cli;
mod cloudflare;
mod commands;
mod compress;
mod config;
mod crawl;
mod doctor;
mod fetch;
mod fetch_utils;
mod init;
mod llm;
mod search;
mod stats;
mod stats_history;
mod storage;
mod subprocess;
mod terminal;
mod text;
mod types;
mod wrap;

use reqwest::blocking::Client;
use std::io::Read;

use crate::bench::run_bench;
use crate::commands::{run_fetch, run_github, run_repo, run_search, run_summarize};
use crate::compress::run_compress;
use crate::config::{print_config, resolve_config, run_setup};
use crate::crawl::run_crawl;
use crate::init::run_init;
use crate::stats::{print_stats, print_stats_json};
use crate::wrap::run_wrap;

use cli::{Cli, Commands};
use config::Config;

fn main() {
    let cli = cli::parse();
    match &cli.command {
        Commands::Stats if cli.json => print_stats_json(),
        Commands::Stats => print_stats(),
        Commands::ResetStats => {
            stats_history::reset_stats().unwrap_or_else(|error| commands::fail(1, &error));
            println!("Statistics reset.");
        }
        Commands::ResetCache => println!("Cleared {} cached entries.", cache::cache_clear()),
        Commands::Config => configuration_or_exit(print_config()),
        Commands::Doctor { check_hosted } => {
            let cfg = configuration_or_exit(resolve_config(&cli));
            doctor::run(&cli, &cfg, *check_hosted);
        }
        Commands::Init {
            uninstall,
            with_repomix,
        } => run_init(*uninstall, *with_repomix),
        Commands::Setup { openrouter_key } => {
            configuration_or_exit(run_setup(openrouter_key.as_deref()))
        }
        Commands::Wrap { command } => run_wrap(command),
        Commands::Compress { max_chars, source } => run_compress(*max_chars, source.as_deref()),
        Commands::Summarize { prompt } => {
            let cfg = if cli.raw {
                Config::default()
            } else {
                configuration_or_exit(resolve_config(&cli))
            };
            run_summarize(&cli, &cfg, &prompt.join(" "));
        }
        _ => run_acquisition(&cli, &configuration_or_exit(resolve_config(&cli))),
    }
}

fn run_acquisition(cli: &Cli, cfg: &Config) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("HTTP client");
    match &cli.command {
        Commands::Search { query } => {
            let query = query.join(" ");
            if query.is_empty() {
                eprintln!("Usage: ai-summary <query>");
                std::process::exit(1);
            }
            run_search(cli, cfg, &client, &query);
        }
        Commands::Fetch { urls, prompt } => {
            let urls = read_urls(urls);
            run_fetch(cli, cfg, &client, &urls, prompt.as_deref());
        }
        Commands::Bench => run_bench(cfg, &client),
        Commands::Crawl {
            url,
            prompt,
            limit,
            depth,
        } => {
            run_crawl(cli, cfg, url, prompt.as_deref(), *limit, *depth);
        }
        Commands::Github {
            args,
            repo,
            language,
            prompt,
        } => {
            run_github(cli, cfg, args, repo, language, prompt);
        }
        Commands::Repo {
            repo,
            prompt,
            include,
        } => {
            run_repo(cli, cfg, repo, prompt, include.as_deref());
        }
        _ => unreachable!("only acquisition commands reach this dispatcher"),
    }
}

fn read_urls(args: &[String]) -> Vec<String> {
    if !args.is_empty() {
        return check_urls(args.to_vec());
    }
    let input = if terminal::is_stdin_terminal() {
        String::new()
    } else {
        let bytes = read_limit(std::io::stdin().lock(), 256 * 1024)
            .unwrap_or_else(|error| commands::fail(1, &error));
        String::from_utf8(bytes).unwrap_or_else(|_| commands::fail(1, "URL list is not UTF-8"))
    };
    let urls: Vec<_> = input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    check_urls(urls)
}

fn check_urls(urls: Vec<String>) -> Vec<String> {
    if urls.is_empty() {
        commands::fail(1, "no URLs provided");
    }
    if urls.len() > 128 {
        commands::fail(1, "fetch accepts at most 128 URLs");
    }
    urls
}

fn configuration_or_exit<T>(result: Result<T, String>) -> T {
    result.unwrap_or_else(|error| {
        eprintln!("[ai-summary] {error}");
        std::process::exit(1);
    })
}

pub(crate) fn read_limit(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read input")?;
    if bytes.len() as u64 > limit {
        return Err(format!("input exceeds {limit} bytes"));
    }
    Ok(bytes)
}

pub(crate) fn credential_client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "cannot initialize HTTP client".into())
}
