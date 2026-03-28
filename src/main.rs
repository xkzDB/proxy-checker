mod checker;
mod parser;
mod ui;

use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Parser;
use futures::stream::{self, StreamExt};

use checker::check_proxy;
use parser::{parse_proxy, Protocol};
use ui::{build_progress, print_banner, print_summary};

// ── CLI definition ────────────────────────────────────────────────────────────

/// All-round proxy checker — fast, reliable, multi-threaded.
#[derive(Parser, Debug)]
#[command(
    name = "proxy-checker",
    version,
    about = "Fast, reliable, multi-threaded proxy checker supporting HTTP, SOCKS4, SOCKS4a, SOCKS5"
)]
struct Cli {
    /// Path to the input file (one proxy per line).
    #[arg(short = 'i', long = "input", required = true)]
    input: PathBuf,

    /// Output file for alive proxies.
    #[arg(short = 'o', long = "output", default_value = "alive.txt")]
    output: PathBuf,

    /// Number of concurrent threads/workers (max: 200).
    #[arg(short = 't', long = "threads", default_value_t = 30, value_parser = validate_threads)]
    threads: usize,

    /// Default protocol when lines in the input file have no scheme prefix.
    /// One of: http, socks4, socks4a, socks5.
    #[arg(long = "protocol", value_parser = parse_protocol_arg)]
    protocol: Option<Protocol>,

    /// Per-proxy connection timeout in seconds (default: 10).
    #[arg(long = "timeout", default_value_t = 10)]
    timeout: u64,
}

fn validate_threads(s: &str) -> Result<usize, String> {
    let n: usize = s
        .parse()
        .map_err(|_| format!("'{}' is not a valid number", s))?;
    if n == 0 {
        return Err("threads must be at least 1".to_string());
    }
    if n > 200 {
        return Err("threads cannot exceed 200".to_string());
    }
    Ok(n)
}

fn parse_protocol_arg(s: &str) -> Result<Protocol, String> {
    Protocol::from_str(s).map_err(|e| e.to_string())
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Print banner.
    print_banner();

    // ── Read & parse input file ───────────────────────────────────────────────
    let raw =
        fs::read_to_string(&cli.input).with_context(|| format!("reading {:?}", cli.input))?;

    let default_protocol = cli.protocol.as_ref();

    let mut proxies = Vec::new();
    let mut parse_errors = 0usize;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_proxy(line, default_protocol) {
            Ok(p) => proxies.push(p),
            Err(e) => {
                eprintln!("  [WARN] Skipping '{}': {}", line, e);
                parse_errors += 1;
            }
        }
    }

    if proxies.is_empty() {
        eprintln!("No valid proxies found in {:?}. Exiting.", cli.input);
        if parse_errors > 0 {
            eprintln!(
                "({} lines could not be parsed — did you forget --protocol?)",
                parse_errors
            );
        }
        return Ok(());
    }

    println!(
        "  Loaded {} proxies ({} skipped). Checking with {} workers, {}s timeout...\n",
        proxies.len(),
        parse_errors,
        cli.threads,
        cli.timeout
    );

    // ── Set up progress UI ────────────────────────────────────────────────────
    let total = proxies.len() as u64;
    let (mp, total_bar, thread_bar) = build_progress(total, cli.threads);
    let timeout = Duration::from_secs(cli.timeout);

    // Shared counters.
    let alive_count = Arc::new(AtomicUsize::new(0));
    let active_count = Arc::new(AtomicUsize::new(0));

    // Collect results so we can write them at the end.
    let alive_count_clone = Arc::clone(&alive_count);
    let active_count_clone = Arc::clone(&active_count);
    let total_bar_clone = total_bar.clone();
    let thread_bar_clone = thread_bar.clone();

    // ── Run checks concurrently ───────────────────────────────────────────────
    let results: Vec<_> = stream::iter(proxies)
        .map(|proxy| {
            let alive_count = Arc::clone(&alive_count_clone);
            let active_count = Arc::clone(&active_count_clone);
            let total_bar = total_bar_clone.clone();
            let thread_bar = thread_bar_clone.clone();

            async move {
                // Track active workers.
                let prev = active_count.fetch_add(1, Ordering::Relaxed);
                thread_bar.set_position((prev + 1) as u64);

                let result = check_proxy(proxy, timeout).await;

                let after = active_count.fetch_sub(1, Ordering::Relaxed);
                thread_bar.set_position(after.saturating_sub(1) as u64);

                if result.alive {
                    let n = alive_count.fetch_add(1, Ordering::Relaxed) + 1;
                    total_bar.set_message(format!("{} alive", n));
                }
                total_bar.inc(1);

                result
            }
        })
        .buffer_unordered(cli.threads)
        .collect()
        .await;

    total_bar.finish_with_message(format!("{} alive", alive_count.load(Ordering::Relaxed)));
    thread_bar.finish_and_clear();
    mp.clear()?;

    // ── Write results ─────────────────────────────────────────────────────────
    let alive_proxies: Vec<_> = results.iter().filter(|r| r.alive).collect();
    let alive_total = alive_proxies.len();
    let dead_total = results.len() - alive_total;

    let mut out =
        fs::File::create(&cli.output).with_context(|| format!("creating {:?}", cli.output))?;

    for r in &alive_proxies {
        let line = match r.latency_ms {
            Some(ms) => format!("{} # {}ms\n", r.proxy.to_url(), ms),
            None => format!("{}\n", r.proxy.to_url()),
        };
        out.write_all(line.as_bytes())?;
    }

    // ── Print summary ─────────────────────────────────────────────────────────
    print_summary(
        results.len(),
        alive_total,
        dead_total,
        &cli.output.display().to_string(),
    );

    Ok(())
}
