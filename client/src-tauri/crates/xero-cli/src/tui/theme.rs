//! Xero Dusk color tokens — the only place that touches `Color::Rgb` for the TUI.
//!
//! Values mirror `client/src/features/theme/theme-definitions.ts` so the terminal
//! and desktop builds feel like the same product.

use ratatui::style::{Color, Style};

pub const STRIPE_GLYPH: &str = "\u{258E}"; // ▎
pub const TOOL_DOT: &str = "\u{25AA}"; // ▪

/// How many points to lift each RGB channel of [`BG`] for the composer
/// surface — small enough to read as a subtle elevation, large enough to
/// pop on every reasonable terminal.
const COMPOSER_BG_LIFT: u8 = 0x10;

pub const BG: Color = Color::Rgb(0x12, 0x12, 0x12);
pub const FG: Color = Color::Rgb(0xf8, 0xf9, 0xfa);
pub const MUTED: Color = Color::Rgb(0xa8, 0xae, 0xb5);
pub const DIM: Color = Color::Rgb(0x6b, 0x6f, 0x74);
pub const ACCENT: Color = Color::Rgb(0xd4, 0xa5, 0x74);
pub const PAID: Color = Color::Rgb(0xf5, 0xb9, 0x62);
#[allow(dead_code)]
pub const SUCCESS: Color = Color::Rgb(0x4a, 0xde, 0x80);
#[allow(dead_code)]
pub const ERROR: Color = Color::Rgb(0xef, 0x44, 0x44);
#[allow(dead_code)]
pub const INFO: Color = Color::Rgb(0x7c, 0xc1, 0xe8);

pub fn base() -> Style {
    Style::default().fg(FG).bg(BG)
}

/// Slightly elevated surface for the composer block. The lift is derived
/// from [`BG`] so palette changes flow through automatically.
pub fn composer_bg() -> Style {
    Style::default().fg(FG).bg(composer_bg_color())
}

pub fn composer_bg_color() -> Color {
    lifted(BG, COMPOSER_BG_LIFT)
}

fn lifted(color: Color, amount: u8) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(amount),
            g.saturating_add(amount),
            b.saturating_add(amount),
        ),
        other => other,
    }
}

pub fn fg() -> Style {
    Style::default().fg(FG)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn paid() -> Style {
    Style::default().fg(PAID)
}

#[allow(dead_code)]
pub fn error() -> Style {
    Style::default().fg(ERROR)
}
