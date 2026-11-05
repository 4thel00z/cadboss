//! Colors from the cadboss logo on the terminal's own background. Terminals
//! without truecolor get the nearest xterm-256 entries.

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Focus border, selection gutter, chips, keys, match highlight, cursor.
    pub accent: Color,
    /// Text on an accent background.
    pub on_accent: Color,
    /// Problem toasts.
    pub error: Color,
    /// Skipped count and the frozen, off and locked flags.
    pub warning: Color,
    /// Type group names and the entity type in rows.
    pub info: Color,
    /// Success toasts.
    pub ok: Color,
    /// Labels, counts, group codes, fold markers.
    pub muted: Color,
    /// Unfocused borders, guides, rules, disabled copy lines.
    pub border: Color,
    /// The selected row's band.
    pub selection: Color,
    /// Numeric values and the ASCII column.
    pub number: Color,
}

impl Theme {
    pub fn truecolor() -> Theme {
        Theme {
            accent: Color::Rgb(0xEB, 0x68, 0x34),
            on_accent: Color::Rgb(0x1A, 0x1A, 0x1A),
            error: Color::Rgb(0xE8, 0x45, 0x3C),
            warning: Color::Rgb(0xF9, 0xA8, 0x00),
            info: Color::Rgb(0x4A, 0x8F, 0xD9),
            ok: Color::Rgb(0x57, 0xA6, 0x39),
            muted: Color::Rgb(0x8A, 0x8D, 0x86),
            border: Color::Rgb(0x4A, 0x4D, 0x48),
            selection: Color::Rgb(0x33, 0x30, 0x2B),
            number: Color::Rgb(0xD7, 0xB5, 0x6D),
        }
    }

    pub fn indexed() -> Theme {
        Theme {
            accent: Color::Indexed(202),
            on_accent: Color::Indexed(234),
            error: Color::Indexed(203),
            warning: Color::Indexed(214),
            info: Color::Indexed(68),
            ok: Color::Indexed(71),
            muted: Color::Indexed(245),
            border: Color::Indexed(239),
            selection: Color::Indexed(236),
            number: Color::Indexed(179),
        }
    }

    /// Truecolor when `COLORTERM` says so, the 256-color palette otherwise.
    pub fn detect() -> Theme {
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        if !matches!(colorterm.as_str(), "truecolor" | "24bit") {
            return Theme::indexed();
        }
        Theme::truecolor()
    }

    pub fn fg(&self, color: Color) -> Style {
        Style::default().fg(color)
    }

    pub fn muted(&self) -> Style {
        self.fg(self.muted)
    }

    pub fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    /// A key in the hint bar.
    pub fn key(&self) -> Style {
        self.fg(self.accent).add_modifier(Modifier::BOLD)
    }

    /// A filled label, `on_accent` text on `color`.
    pub fn chip(&self, color: Color) -> Style {
        Style::default()
            .fg(self.on_accent)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    }
}

impl Default for Theme {
    fn default() -> Theme {
        Theme::truecolor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_keep_colors_distinct() {
        for theme in [Theme::truecolor(), Theme::indexed()] {
            assert_ne!(theme.accent, theme.error);
            assert_ne!(theme.error, theme.warning);
            assert_ne!(theme.border, theme.selection);
            assert_ne!(theme.accent, theme.info);
        }
    }
}
