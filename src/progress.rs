//! Progress bars for the slow, countable phases — cloning many repositories
//! and the parallel GitHub enrichment passes — built on `indicatif`. The
//! styling mirrors Seqera's RustQC (cyan braille spinner, a filled bar, and a
//! dim elapsed time) so the two tools feel consistent.
//!
//! Bars draw to stderr. `indicatif` hides them automatically when stderr is not
//! a terminal, so piped and CI output stays clean; callers additionally pass
//! `show` (the verbose flag) to opt out entirely. When hidden, the returned bar
//! is a no-op, so `inc`/`finish_and_clear` are safe to call unconditionally.

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Braille spinner frames, matching RustQC's progress style.
const TICKS: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷", "⣾"];

/// A determinate progress bar over `total` items, rendered as
/// `⣾ label [████░░░░] 12/57 3s`. Returns a hidden (no-op) bar when `show` is
/// false or there is nothing to count, so callers don't need to branch.
pub fn bar(label: &str, total: usize, show: bool) -> ProgressBar {
    if !show || total == 0 {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "  {spinner:.cyan} {msg} [{bar:24.cyan/dim}] {pos}/{len} {elapsed:.dim}",
        )
        .expect("valid progress template")
        .progress_chars("█▉▊▋▌▍▎▏ ")
        .tick_strings(TICKS),
    );
    pb.set_message(label.to_string());
    // Animate the spinner even while a single long item (e.g. a big clone) is in
    // flight and the count isn't moving.
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}
