use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// ASCII art banner for "xkzDB".
pub const BANNER: &str = r"
           /$$                 /$$$$$$$  /$$$$$$$ 
          | $$                | $$__  $$| $$__  $$
 /$$   /$$| $$   /$$ /$$$$$$$$| $$  \ $$| $$  \ $$
|  $$ /$$/| $$  /$$/|____ /$$/| $$  | $$| $$$$$$$ 
 \  $$$$/ | $$$$$$/    /$$$$/ | $$  | $$| $$__  $$
  >$$  $$ | $$_  $$   /$$__/  | $$  | $$| $$  \ $$
 /$$/\  $$| $$ \  $$ /$$$$$$$$| $$$$$$$/| $$$$$$$/
|__/  \__/|__/  \__/|________/|_______/ |_______/ 
";

/// Print the banner to stdout with colour.
pub fn print_banner() {
    println!("{}", BANNER.bright_cyan().bold());
    println!(
        "{}",
        "  All-round Proxy Checker — fast, reliable, multi-threaded"
            .bright_white()
            .dimmed()
    );
    println!();
}

/// Create and return the shared `MultiProgress` container plus the two
/// progress bars used during checking:
///
/// * `total_bar`   – overall completion bar
/// * `thread_bar`  – spinner showing currently active workers
pub fn build_progress(total: u64, threads: usize) -> (MultiProgress, ProgressBar, ProgressBar) {
    let mp = MultiProgress::new();

    // ── Overall progress bar ──────────────────────────────────────────────────
    let total_bar = mp.add(ProgressBar::new(total));
    total_bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:50.cyan/blue}] {pos}/{len} ({percent}%) | ✅ {msg}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏ "),
    );
    total_bar.set_message("0 alive");

    // ── Active-threads counter ────────────────────────────────────────────────
    let thread_bar = mp.add(ProgressBar::new(threads as u64));
    thread_bar.set_style(
        ProgressStyle::with_template(
            "  {spinner:.yellow} Active workers: {pos}/{len}  (max {len})",
        )
        .unwrap(),
    );
    thread_bar.set_position(0);

    (mp, total_bar, thread_bar)
}

/// Print a summary line after checking is done.
pub fn print_summary(total: usize, alive: usize, dead: usize, output: &str) {
    println!();
    println!("{}", "─".repeat(60).bright_black());
    println!(
        "  {}  Total checked : {}",
        "📊".bright_white(),
        total.to_string().bright_white().bold()
    );
    println!(
        "  {}  Alive proxies : {}",
        "✅".bright_white(),
        alive.to_string().bright_green().bold()
    );
    println!(
        "  {}  Dead proxies  : {}",
        "❌".bright_white(),
        dead.to_string().bright_red().bold()
    );
    println!(
        "  {}  Results saved : {}",
        "💾".bright_white(),
        output.bright_yellow()
    );
    println!("{}", "─".repeat(60).bright_black());
    println!();
}
