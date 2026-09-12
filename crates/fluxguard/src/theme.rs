//! Terminal presentation for the interactive paths: the brand mark, a few
//! coloured line helpers, and the prompt styling.
//!
//! Everything degrades to plain text when stdout is not a terminal or
//! `NO_COLOR` is set, so piped output stays readable.

use std::io::IsTerminal;

use inquire::ui::{Attributes, Color, RenderConfig, StyleSheet, Styled};

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

/// The shield-and-flux mark from `assets/brand/logo-mark.svg`, sampled to
/// twelve by twelve. `g` is the shield, `d` the flux line struck through it, a
/// space is outside the mark. Regenerate with `sh tests/logo.sh`.
const LOGO: [&str; 12] = [
    "gggggggggggg",
    "gggggggggggg",
    "gggggggggggg",
    "ggggddgggggg",
    "ggggddgggggg",
    "gggdddggdggg",
    "ggggggdddggg",
    "ggggggddgggg",
    " gggggdgggg ",
    "  gggggggg  ",
    "   gggggg   ",
    "     gg     ",
];
const SHIELD: &str = "\x1b[38;5;114m";
const SHIELD_BG: &str = "\x1b[48;5;114m";
const STRUCK: &str = "\x1b[38;5;234m";
const STRUCK_BG: &str = "\x1b[48;5;234m";
const DEFAULT_BG: &str = "\x1b[49m";

/// Draw the mark two pixel rows per line: `▀` paints the upper half in the
/// foreground and the lower half in the background, so a text cell carries two
/// pixels. Anything outside the mark keeps the terminal's own background rather
/// than punching a coloured hole in it.
fn logo_rows() -> Vec<String> {
    let cell = |upper: u8, lower: u8| match (upper, lower) {
        (b' ', b' ') => " ".to_owned(),
        (b' ', lower) => {
            let colour = if lower == b'g' { SHIELD } else { STRUCK };
            format!("{colour}{DEFAULT_BG}▄{RESET}")
        }
        (upper, b' ') => {
            let colour = if upper == b'g' { SHIELD } else { STRUCK };
            format!("{colour}{DEFAULT_BG}▀{RESET}")
        }
        (upper, lower) => {
            let top = if upper == b'g' { SHIELD } else { STRUCK };
            let bottom = if lower == b'g' { SHIELD_BG } else { STRUCK_BG };
            format!("{top}{bottom}▀{RESET}")
        }
    };
    LOGO.chunks(2)
        .map(|pair| {
            let upper = pair[0].as_bytes();
            let lower = pair[1].as_bytes();
            (0..upper.len())
                .map(|column| cell(upper[column], lower[column]))
                .collect()
        })
        .collect()
}

/// The mark beside the wordmark, split the way the logo splits it: `Flux`
/// plain, `Guard` in the accent. Falls back to plain text whenever colour is
/// off, because the mark is made of colour and would otherwise be a smear of
/// half-blocks.
pub fn banner() -> String {
    if !enabled() {
        return "FluxGuard".to_owned();
    }
    let wordmark = format!("{BOLD_FG}Flux{RESET}{ACCENT}Guard{RESET}");
    logo_rows()
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            if index == 2 {
                format!("  {row}   {wordmark}")
            } else {
                format!("  {row}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// Prompt styling that matches the banner: accent for the answered value and
/// the selection cursor, dimmed help text.
pub fn render_config() -> RenderConfig<'static> {
    RenderConfig::default()
        .with_prompt_prefix(Styled::new("◇").with_fg(Color::LightGreen))
        .with_answered_prompt_prefix(Styled::new("◆").with_fg(Color::LightGreen))
        .with_highlighted_option_prefix(Styled::new("›").with_fg(Color::LightGreen))
        .with_answer(
            StyleSheet::new()
                .with_fg(Color::LightGreen)
                .with_attr(Attributes::BOLD),
        )
        .with_help_message(StyleSheet::new().with_fg(Color::DarkGrey))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_grid_is_twelve_by_twelve() {
        assert_eq!(LOGO.len(), 12);
        for row in LOGO {
            assert_eq!(row.chars().count(), 12, "row {row:?}");
            assert!(row.chars().all(|cell| matches!(cell, 'g' | 'd' | ' ')));
        }
    }

    #[test]
    fn logo_renders_two_pixel_rows_per_line() {
        // Six text rows for twelve pixel rows, each carrying every column.
        let rows = logo_rows();
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().all(|row| !row.is_empty()));
    }

    #[test]
    fn plain_output_carries_no_escape_codes() {
        // Tests do not run on a terminal, so every helper must stay plain.
        assert_eq!(banner(), "FluxGuard");
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
