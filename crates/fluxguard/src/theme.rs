//! Terminal presentation shared by the commands that report progress.
//!
//! Everything degrades to plain text when stdout is not a terminal or
//! `NO_COLOR` is set, so piped and redirected output stays readable.

use std::io::IsTerminal;

pub const ACCENT: &str = "\x1b[38;5;114m"; // #4ade80
pub const AMBER: &str = "\x1b[38;5;221m"; // #fbbf24
const BOLD_FG: &str = "\x1b[1;38;5;255m"; // #fafafa
const RESET: &str = "\x1b[0m";

fn enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn paint(color: &str, text: &str) -> String {
    if enabled() {
        format!("{color}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

/// A row that sits on the connector column, marker at column zero.
pub fn point(text: &str, color: &str) -> String {
    paint(color, &format!("◇ {text}"))
}

/// One step of the flow: a labeled rule, then body lines under a shared gutter.
pub fn step(label: &str, lines: &[String], color: &str) -> String {
    let rule = "─".repeat(30usize.saturating_sub(label.len()).max(3));
    let mut out = vec![point(&format!("{label} {rule}"), color), gutter("", color)];
    for line in lines {
        out.push(gutter(&paint(BOLD_FG, line), color));
    }
    out.push(gutter("", color));
    out.join("\n")
}

fn gutter(text: &str, color: &str) -> String {
    let bar = paint(color, "│");
    if text.is_empty() {
        bar
    } else {
        format!("{bar} {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_output_carries_no_escape_codes() {
        // Tests do not run on a terminal, so every helper must stay plain.
        assert_eq!(point("ready", ACCENT), "◇ ready");
        let block = step("update", &["0.1.3 → 0.2.0".into()], ACCENT);
        assert!(!block.contains('\x1b'), "{block:?}");
        assert!(block.contains("0.1.3 → 0.2.0"));
    }

    #[test]
    fn step_rule_never_collapses_for_a_long_label() {
        let block = step("a-very-long-step-label-beyond-thirty", &[], AMBER);
        assert!(block.contains("───"));
    }
}
