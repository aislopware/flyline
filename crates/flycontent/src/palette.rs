use clap_complete::CompletionCandidate;
use ratatui::style::{Color, Modifier, Style};
use std::collections::HashSet;
use strum::{EnumIter, EnumMessage, IntoEnumIterator};

/// Terminal default foreground/background RGB colours probed via OSC 10 / OSC 11.
///
/// Used to detect whether the host terminal is using a dark or light theme,
/// harmonize the autocomplete selection pill with the terminal's true canvas
/// colour, and fade the animated cursor and loading shimmer cleanly into the
/// terminal background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalColors {
    /// Default text foreground colour reported by OSC 10, if known.
    pub fg: Option<(u8, u8, u8)>,
    /// Default text background colour reported by OSC 11, if known.
    pub bg: Option<(u8, u8, u8)>,
}

impl TerminalColors {
    /// Returns `true` if `rgb` has Rec.709 luma ≥ 128 (i.e. a light colour).
    pub fn is_light_rgb((r, g, b): (u8, u8, u8)) -> bool {
        let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
        luma >= 128.0
    }

    /// Linearly blend two RGB triples by `t` ∈ `[0.0, 1.0]`.
    pub fn blend_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
        let t = t.clamp(0.0, 1.0);
        let lerp = |x: u8, y: u8| -> u8 {
            (f32::from(x) + (f32::from(y) - f32::from(x)) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        (lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
    }

    /// Infer whether the terminal is dark or light from its reported OSC 11 background.
    pub fn detected_theme(&self) -> Option<ColourTheme> {
        self.bg.map(|bg| {
            if Self::is_light_rgb(bg) {
                ColourTheme::Light
            } else {
                ColourTheme::Dark
            }
        })
    }

    /// Compute a subtle elevated selection-pill background blended directly from
    /// the terminal's own OSC 11 background toward its OSC 10 foreground, so the
    /// highlight matches the terminal theme's exact hue and temperature.
    pub fn popup_selected_bg(&self) -> Option<Color> {
        let bg = self.bg?;
        let is_light = Self::is_light_rgb(bg);
        let target_fg = self
            .fg
            .unwrap_or(if is_light { (0, 0, 0) } else { (255, 255, 255) });
        let factor = if is_light { 0.10 } else { 0.14 };
        let (r, g, b) = Self::blend_rgb(bg, target_fg, factor);
        Some(Color::Rgb(r, g, b))
    }

    /// Effective canvas background RGB (OSC 11 if known, otherwise theme fallback).
    pub fn effective_bg(&self, theme: ColourTheme) -> (u8, u8, u8) {
        self.bg.unwrap_or(match theme {
            ColourTheme::Dark => (0, 0, 0),
            ColourTheme::Light => (255, 255, 255),
        })
    }

    /// Effective text foreground RGB (OSC 10 if known, otherwise theme fallback).
    pub fn effective_fg(&self, theme: ColourTheme) -> (u8, u8, u8) {
        self.fg.unwrap_or(match theme {
            ColourTheme::Dark => (255, 255, 255),
            ColourTheme::Light => (0, 0, 0),
        })
    }
}

/// Which theme the user has configured for the colour palette.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ColourTheme {
    /// Dark-terminal preset (the original flyline palette). This is the default.
    #[default]
    Dark,
    /// Light-terminal preset.
    Light,
}

/// Configures the visual style of the text buffer cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CursorStyleConfig {
    /// Standard blinking box cursor using terminal inversion.
    #[default]
    Default,
    /// Invert cell background/foreground.
    Reverse,
    /// Custom Ratatui style for the cursor cell.
    Custom(Style),
}

/// Visual interaction state for an interactive button-like cell
/// (clipboard slots, the PS1 copy-buffer button, tutorial buttons, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    /// No mouse interaction with the cell.
    Normal,
    /// The mouse cursor is hovering over the cell.
    Hovered,
    /// The mouse cursor is hovering over the cell and the left mouse button
    /// is currently held down.
    Depressed,
}

/// Parse a rich-style string (e.g. `"bold red"`) into a `ratatui::style::Style`.
/// Returns an error message if the string cannot be parsed.
pub fn parse_str_to_style(s: &str) -> Result<ratatui::style::Style, String> {
    use parse_style::{Attribute, Style as ParseStyle};
    use ratatui::style::{Modifier, Style};

    let parsed: ParseStyle = s.parse().map_err(|e| format!("{e}"))?;
    let mut style = Style::default();

    if let Some(fg) = parsed.get_foreground() {
        style = style.fg(parse_color_to_ratatui(fg));
    }
    if let Some(bg) = parsed.get_background() {
        style = style.bg(parse_color_to_ratatui(bg));
    }

    let attr_map: &[(Attribute, Modifier)] = &[
        (Attribute::Bold, Modifier::BOLD),
        (Attribute::Dim, Modifier::DIM),
        (Attribute::Italic, Modifier::ITALIC),
        (Attribute::Underline, Modifier::UNDERLINED),
        (Attribute::Blink, Modifier::SLOW_BLINK),
        (Attribute::Blink2, Modifier::RAPID_BLINK),
        (Attribute::Reverse, Modifier::REVERSED),
        (Attribute::Conceal, Modifier::HIDDEN),
        (Attribute::Strike, Modifier::CROSSED_OUT),
    ];
    for &(attr, modifier) in attr_map {
        if parsed.is_enabled(attr) {
            style = style.add_modifier(modifier);
        }
    }
    Ok(style)
}

/// Parse a cursor style string into a [`CursorStyleConfig`].
///
/// Special values:
/// - `"reverse"` (case-insensitive): returns [`CursorStyleConfig::Reverse`].
/// - `"default"` (case-insensitive): returns [`CursorStyleConfig::Default`].
///
/// Otherwise the string is parsed as a rich-style expression with one difference
/// from [`parse_str_to_style`]: a **single colour with no `on` keyword** is
/// treated as the **background** colour of the cursor cell (e.g. `"red"` →
/// `bg(red)`).  When an explicit `on` is present (e.g. `"pink on white"`) the
/// foreground and background are used as-is.
pub fn parse_cursor_style_str(s: &str) -> Result<CursorStyleConfig, String> {
    use parse_style::{Attribute, Style as ParseStyle};
    use ratatui::style::Modifier;

    if s.eq_ignore_ascii_case("reverse") {
        return Ok(CursorStyleConfig::Reverse);
    }
    if s.eq_ignore_ascii_case("default") {
        return Ok(CursorStyleConfig::Default);
    }

    let parsed: ParseStyle = s.parse().map_err(|e| format!("{e}"))?;
    let mut style = Style::default();

    match (parsed.get_foreground(), parsed.get_background()) {
        (None, None) => {}
        // Single colour → treat as background
        (Some(fg), None) => {
            style = style.bg(parse_color_to_ratatui(fg));
        }
        (fg, Some(bg)) => {
            if let Some(f) = fg {
                style = style.fg(parse_color_to_ratatui(f));
            }
            style = style.bg(parse_color_to_ratatui(bg));
        }
    }

    let attr_map: &[(Attribute, Modifier)] = &[
        (Attribute::Bold, Modifier::BOLD),
        (Attribute::Dim, Modifier::DIM),
        (Attribute::Italic, Modifier::ITALIC),
        (Attribute::Underline, Modifier::UNDERLINED),
        (Attribute::Blink, Modifier::SLOW_BLINK),
        (Attribute::Blink2, Modifier::RAPID_BLINK),
        (Attribute::Reverse, Modifier::REVERSED),
        (Attribute::Conceal, Modifier::HIDDEN),
        (Attribute::Strike, Modifier::CROSSED_OUT),
    ];
    for &(attr, modifier) in attr_map {
        if parsed.is_enabled(attr) {
            style = style.add_modifier(modifier);
        }
    }
    Ok(CursorStyleConfig::Custom(style))
}

fn parse_color_to_ratatui(c: parse_style::Color) -> ratatui::style::Color {
    use parse_style::Color;
    match c {
        Color::Default => ratatui::style::Color::Reset,
        Color::Color256(c256) => ratatui::style::Color::Indexed(c256.0),
        Color::Rgb(rgb) => ratatui::style::Color::Rgb(rgb.red(), rgb.green(), rgb.blue()),
    }
}

/// All individually-configurable palette slots.
///
/// The kebab-case name of each variant (e.g. `"recognised-command"`) is used
/// in the `flyline set-style NAME=STYLE` command-line interface.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    EnumMessage,
    strum::Display,
    strum::EnumString,
)]
#[strum(serialize_all = "kebab-case")]
pub enum PaletteStyleKind {
    #[strum(message = "Syntax highlighting for recognised shell commands (e.g. ls, git)")]
    RecognisedCommand,
    #[strum(message = "Syntax highlighting for unrecognised commands")]
    UnrecognisedCommand,
    #[strum(message = "Syntax highlighting for single-quoted strings")]
    SingleQuotedText,
    #[strum(message = "Syntax highlighting for double-quoted strings")]
    DoubleQuotedText,
    #[strum(message = "Dimmed style for secondary and decorative text")]
    SecondaryText,
    #[strum(message = "Style for inline history suggestions shown after the cursor")]
    InlineSuggestion,
    #[strum(message = "Style for tutorial hint text")]
    TutorialHint,
    #[strum(message = "Highlight style for characters matched by fuzzy search")]
    MatchingChar,
    #[strum(message = "Style for matched opening/closing bracket or quote pairs")]
    OpeningAndClosingPair,
    #[strum(message = "Default style for unclassified command buffer text")]
    NormalText,
    #[strum(message = "Syntax highlighting for shell comments (text after #)")]
    Comment,
    #[strum(message = "Syntax highlighting for environment variable references (e.g. $HOME)")]
    EnvVar,
    #[strum(message = "Syntax highlighting for unrecognised environment variable references")]
    UnrecognisedEnvVar,
    #[strum(message = "Style for level-1 Markdown headings (# heading)")]
    MarkdownHeading1,
    #[strum(message = "Style for level-2 Markdown headings (## heading)")]
    MarkdownHeading2,
    #[strum(message = "Style for level-3 Markdown headings (### heading)")]
    MarkdownHeading3,
    #[strum(message = "Style for inline code spans in Markdown")]
    MarkdownCode,
    #[strum(message = "Style used to render key sequences in the UI")]
    KeySequenceStyle,
    #[strum(message = "Highlight style for the current text selection")]
    SelectedText,
    #[strum(message = "Syntax highlighting for bash reserved words (e.g. if, while, for)")]
    BashReserved,
    #[strum(message = "Style for the right click context menu background")]
    RightClickMenu,
    #[strum(message = "Style for the completion menu scrollbar")]
    Scrollbar,
    #[strum(message = "Background surface style for the autocomplete popup")]
    PopupBg,
    #[strum(message = "Style for the autocomplete popup left rail")]
    PopupRail,
    #[strum(message = "Accent style for the selected row indicator in the autocomplete popup")]
    PopupIndicator,
    #[strum(message = "Highlight style for the selected row in the autocomplete popup")]
    PopupSelected,
    #[strum(message = "Style for unselected item descriptions in the autocomplete popup")]
    PopupDescription,
    #[strum(message = "Style for the selected item description in the autocomplete popup")]
    PopupSelectedDescription,
    #[strum(message = "Rainbow bracket/quote colour for nesting depth 1 (outermost)")]
    RainbowBracket1,
    #[strum(message = "Rainbow bracket/quote colour for nesting depth 2")]
    RainbowBracket2,
    #[strum(message = "Rainbow bracket/quote colour for nesting depth 3")]
    RainbowBracket3,
    #[strum(message = "Rainbow bracket/quote colour for nesting depth 4")]
    RainbowBracket4,
}

/// The colour palette.  One [`Style`] per slot.
///
/// Use [`Palette::apply_theme`] to reset all slots from a built-in preset,
/// then call [`Palette::set`] (or set the public fields directly) to customise
/// individual slots.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Palette {
    recognised_command: Style,
    unrecognised_command: Style,
    single_quoted_text: Style,
    double_quoted_text: Style,
    secondary_text: Style,
    inline_suggestion: Style,
    tutorial_hint: Style,
    matching_char: Style,
    opening_and_closing_pair: Style,
    normal_text: Style,
    comment: Style,
    env_var: Style,
    unrecognised_env_var: Style,
    markdown_heading1: Style,
    markdown_heading2: Style,
    markdown_heading3: Style,
    markdown_code: Style,
    key_sequence_style: Style,
    selected_text: Style,
    bash_reserved: Style,
    right_click_menu: Style,
    scrollbar: Style,
    popup_bg: Style,
    popup_rail: Style,
    popup_indicator: Style,
    popup_selected: Style,
    popup_description: Style,
    popup_selected_description: Style,
    rainbow_brackets: [Style; 4],
}

impl Palette {
    // ── Getters ───────────────────────────────────────────────────────

    pub fn recognised_command(&self) -> Style {
        self.recognised_command
    }

    pub fn unrecognised_command(&self) -> Style {
        self.unrecognised_command
    }

    pub fn single_quoted_text(&self) -> Style {
        self.single_quoted_text
    }

    pub fn double_quoted_text(&self) -> Style {
        self.double_quoted_text
    }

    pub fn secondary_text(&self) -> Style {
        self.secondary_text
    }

    pub fn inline_suggestion(&self) -> Style {
        self.inline_suggestion
    }

    pub fn tutorial_hint(&self) -> Style {
        self.tutorial_hint
    }

    pub fn matching_char(&self) -> Style {
        self.matching_char
    }

    pub fn opening_and_closing_pair(&self) -> Style {
        self.opening_and_closing_pair
    }

    pub fn normal_text(&self) -> Style {
        self.normal_text
    }

    pub fn comment(&self) -> Style {
        self.comment
    }

    pub fn env_var(&self) -> Style {
        self.env_var
    }

    pub fn unrecognised_env_var(&self) -> Style {
        self.unrecognised_env_var
    }

    pub fn markdown_heading1(&self) -> Style {
        self.markdown_heading1
    }

    pub fn markdown_heading2(&self) -> Style {
        self.markdown_heading2
    }

    pub fn markdown_heading3(&self) -> Style {
        self.markdown_heading3
    }

    pub fn markdown_code(&self) -> Style {
        self.markdown_code
    }

    pub fn key_sequence_style(&self) -> Style {
        self.key_sequence_style
    }

    pub fn selected_text(&self) -> Style {
        self.selected_text
    }

    pub fn bash_reserved(&self) -> Style {
        self.bash_reserved
    }

    pub fn right_click_menu(&self) -> Style {
        self.right_click_menu
    }

    pub fn scrollbar(&self) -> Style {
        self.scrollbar
    }

    pub fn popup_bg(&self) -> Style {
        self.popup_bg
    }

    pub fn popup_rail(&self) -> Style {
        self.popup_rail
    }

    pub fn popup_indicator(&self) -> Style {
        self.popup_indicator
    }

    pub fn popup_selected(&self) -> Style {
        self.popup_selected
    }

    pub fn popup_description(&self) -> Style {
        self.popup_description
    }

    pub fn popup_selected_description(&self) -> Style {
        self.popup_selected_description
    }

    /// Return the rainbow bracket/quote style for the given nesting `depth`.
    /// Cycles through the 4 palette slots using `depth % 4`.
    pub fn rainbow_bracket(&self, depth: usize) -> Style {
        self.rainbow_brackets[depth % 4]
    }

    /// Get an individual palette slot by kind.
    pub fn get(&self, kind: PaletteStyleKind) -> Style {
        match kind {
            PaletteStyleKind::RecognisedCommand => self.recognised_command,
            PaletteStyleKind::UnrecognisedCommand => self.unrecognised_command,
            PaletteStyleKind::SingleQuotedText => self.single_quoted_text,
            PaletteStyleKind::DoubleQuotedText => self.double_quoted_text,
            PaletteStyleKind::SecondaryText => self.secondary_text,
            PaletteStyleKind::InlineSuggestion => self.inline_suggestion,
            PaletteStyleKind::TutorialHint => self.tutorial_hint,
            PaletteStyleKind::MatchingChar => self.matching_char,
            PaletteStyleKind::OpeningAndClosingPair => self.opening_and_closing_pair,
            PaletteStyleKind::NormalText => self.normal_text,
            PaletteStyleKind::Comment => self.comment,
            PaletteStyleKind::EnvVar => self.env_var,
            PaletteStyleKind::UnrecognisedEnvVar => self.unrecognised_env_var,
            PaletteStyleKind::MarkdownHeading1 => self.markdown_heading1,
            PaletteStyleKind::MarkdownHeading2 => self.markdown_heading2,
            PaletteStyleKind::MarkdownHeading3 => self.markdown_heading3,
            PaletteStyleKind::MarkdownCode => self.markdown_code,
            PaletteStyleKind::KeySequenceStyle => self.key_sequence_style,
            PaletteStyleKind::SelectedText => self.selected_text,
            PaletteStyleKind::BashReserved => self.bash_reserved,
            PaletteStyleKind::RightClickMenu => self.right_click_menu,
            PaletteStyleKind::Scrollbar => self.scrollbar,
            PaletteStyleKind::PopupBg => self.popup_bg,
            PaletteStyleKind::PopupRail => self.popup_rail,
            PaletteStyleKind::PopupIndicator => self.popup_indicator,
            PaletteStyleKind::PopupSelected => self.popup_selected,
            PaletteStyleKind::PopupDescription => self.popup_description,
            PaletteStyleKind::PopupSelectedDescription => self.popup_selected_description,
            PaletteStyleKind::RainbowBracket1 => self.rainbow_brackets[0],
            PaletteStyleKind::RainbowBracket2 => self.rainbow_brackets[1],
            PaletteStyleKind::RainbowBracket3 => self.rainbow_brackets[2],
            PaletteStyleKind::RainbowBracket4 => self.rainbow_brackets[3],
        }
    }

    // ── Setter ────────────────────────────────────────────────────────

    /// Set an individual palette slot by kind.
    pub fn set(&mut self, kind: PaletteStyleKind, style: Style) {
        match kind {
            PaletteStyleKind::RecognisedCommand => self.recognised_command = style,
            PaletteStyleKind::UnrecognisedCommand => self.unrecognised_command = style,
            PaletteStyleKind::SingleQuotedText => self.single_quoted_text = style,
            PaletteStyleKind::DoubleQuotedText => self.double_quoted_text = style,
            PaletteStyleKind::SecondaryText => self.secondary_text = style,
            PaletteStyleKind::InlineSuggestion => self.inline_suggestion = style,
            PaletteStyleKind::TutorialHint => self.tutorial_hint = style,
            PaletteStyleKind::MatchingChar => self.matching_char = style,
            PaletteStyleKind::OpeningAndClosingPair => self.opening_and_closing_pair = style,
            PaletteStyleKind::NormalText => self.normal_text = style,
            PaletteStyleKind::Comment => self.comment = style,
            PaletteStyleKind::EnvVar => self.env_var = style,
            PaletteStyleKind::UnrecognisedEnvVar => self.unrecognised_env_var = style,
            PaletteStyleKind::MarkdownHeading1 => self.markdown_heading1 = style,
            PaletteStyleKind::MarkdownHeading2 => self.markdown_heading2 = style,
            PaletteStyleKind::MarkdownHeading3 => self.markdown_heading3 = style,
            PaletteStyleKind::MarkdownCode => self.markdown_code = style,
            PaletteStyleKind::KeySequenceStyle => self.key_sequence_style = style,
            PaletteStyleKind::SelectedText => self.selected_text = style,
            PaletteStyleKind::BashReserved => self.bash_reserved = style,
            PaletteStyleKind::RightClickMenu => self.right_click_menu = style,
            PaletteStyleKind::Scrollbar => self.scrollbar = style,
            PaletteStyleKind::PopupBg => self.popup_bg = style,
            PaletteStyleKind::PopupRail => self.popup_rail = style,
            PaletteStyleKind::PopupIndicator => self.popup_indicator = style,
            PaletteStyleKind::PopupSelected => self.popup_selected = style,
            PaletteStyleKind::PopupDescription => self.popup_description = style,
            PaletteStyleKind::PopupSelectedDescription => self.popup_selected_description = style,
            PaletteStyleKind::RainbowBracket1 => self.rainbow_brackets[0] = style,
            PaletteStyleKind::RainbowBracket2 => self.rainbow_brackets[1] = style,
            PaletteStyleKind::RainbowBracket3 => self.rainbow_brackets[2] = style,
            PaletteStyleKind::RainbowBracket4 => self.rainbow_brackets[3] = style,
        }
    }

    // ── Presets ──────────────────────────────────────────────────────

    /// Dark-terminal defaults (the original flyline palette).
    pub fn dark() -> Self {
        Palette {
            recognised_command: Style::default().fg(Color::Green),
            unrecognised_command: Style::default().fg(Color::Red),
            single_quoted_text: Style::default().fg(Color::Yellow),
            double_quoted_text: Style::default().fg(Color::Magenta),
            secondary_text: Style::default().add_modifier(Modifier::DIM),
            inline_suggestion: Style::default().add_modifier(Modifier::DIM),
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            matching_char: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            opening_and_closing_pair: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            normal_text: Style::default(),
            comment: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::ITALIC),
            env_var: Style::default().fg(Color::Cyan),
            unrecognised_env_var: Style::default().fg(Color::Red),
            markdown_heading1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            markdown_heading2: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            markdown_heading3: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            markdown_code: Style::default().add_modifier(Modifier::DIM),
            key_sequence_style: Style::default().add_modifier(Modifier::DIM),
            selected_text: Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(255, 152, 152)),
            bash_reserved: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            right_click_menu: Style::default().fg(Color::Black).bg(Color::Gray),
            scrollbar: Style::default(),
            popup_bg: Style::default(),
            popup_rail: Style::default().add_modifier(Modifier::DIM),
            popup_indicator: Style::default().fg(Color::Cyan),
            popup_selected: Style::default().bg(Color::DarkGray),
            popup_description: Style::default().add_modifier(Modifier::DIM),
            popup_selected_description: Style::default().add_modifier(Modifier::DIM),
            rainbow_brackets: [
                Style::default().fg(Color::Yellow),
                Style::default().fg(Color::LightRed),
                Style::default().fg(Color::Cyan),
                Style::default().fg(Color::Green),
            ],
        }
    }

    /// Light-terminal defaults.
    pub fn light() -> Self {
        Palette {
            recognised_command: Style::default().fg(Color::Green).bold(),
            unrecognised_command: Style::default().fg(Color::Red).bold(),
            single_quoted_text: Style::default().fg(Color::Magenta),
            double_quoted_text: Style::default().fg(Color::Magenta),
            secondary_text: Style::default().add_modifier(Modifier::DIM),
            inline_suggestion: Style::default().add_modifier(Modifier::DIM),
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            matching_char: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            opening_and_closing_pair: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            normal_text: Style::default(),
            comment: Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
            env_var: Style::default().fg(Color::Blue),
            unrecognised_env_var: Style::default().fg(Color::Red),
            markdown_heading1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            markdown_heading2: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            markdown_heading3: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            markdown_code: Style::default().add_modifier(Modifier::DIM),
            key_sequence_style: Style::default().fg(Color::DarkGray),
            selected_text: Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(255, 102, 102)),
            bash_reserved: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            right_click_menu: Style::default().fg(Color::Black).bg(Color::Gray),
            scrollbar: Style::default(),
            popup_bg: Style::default(),
            popup_rail: Style::default().add_modifier(Modifier::DIM),
            popup_indicator: Style::default().fg(Color::Blue),
            popup_selected: Style::default().bg(Color::Gray),
            popup_description: Style::default().add_modifier(Modifier::DIM),
            popup_selected_description: Style::default().add_modifier(Modifier::DIM),
            rainbow_brackets: [
                Style::default().fg(Color::Yellow),
                Style::default().fg(Color::Green),
                Style::default().fg(Color::Cyan),
                Style::default().fg(Color::Blue),
            ],
        }
    }

    /// Build the preset palette for `mode`.
    pub fn for_theme(mode: ColourTheme) -> Self {
        match mode {
            ColourTheme::Dark => Self::dark(),
            ColourTheme::Light => Self::light(),
        }
    }

    /// Reset all palette slots to the given theme preset.
    pub fn apply_theme(&mut self, mode: ColourTheme) {
        *self = Self::for_theme(mode);
    }

    /// Apply a theme preset to all slots except those explicitly listed in `preserved`.
    pub fn apply_theme_preserving(
        &mut self,
        mode: ColourTheme,
        preserved: &HashSet<PaletteStyleKind>,
    ) {
        let preset = Self::for_theme(mode);
        for kind in PaletteStyleKind::iter() {
            if !preserved.contains(&kind) {
                self.set(kind, preset.get(kind));
            }
        }
    }

    // ── Derived / constant styles ───────────────────────────────────

    pub fn convert_to_highlighted(style: Style) -> Style {
        style.add_modifier(Modifier::REVERSED)
    }

    /// Apply the styling that corresponds to a non-normal [`ButtonState`] on
    /// top of `style`. Callers should branch on [`ButtonState::Normal`]
    /// themselves and only invoke this for `Hovered` or `Depressed`.
    pub fn apply_button_style(style: Style, state: ButtonState) -> Style {
        match state {
            ButtonState::Normal => style,
            ButtonState::Hovered => style.add_modifier(Modifier::REVERSED),
            ButtonState::Depressed => style
                .fg(Color::Black)
                .bg(Color::Rgb(100, 100, 100))
                .add_modifier(Modifier::BOLD),
        }
    }

    pub fn convert_to_selected(&self, style: Style) -> Style {
        style.patch(self.selected_text())
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

/// Tab-completion for the `NAME=STYLE` arguments of `flyline set-style`.
///
/// Yields each [`PaletteStyleKind`] name (in kebab-case) with `=` appended and
/// `NO_SUFFIX` so that flyline's completion engine suppresses the trailing
/// space, allowing the user to type the style value immediately after the `=`.
///
/// When the current token already contains `=` the user is typing the style
/// value (a free-form rich-style string), so no candidates are returned.
pub fn possible_style_name_completions(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy().to_string();
    if current.contains('=') {
        return Vec::new();
    }
    let current_lower = current.to_lowercase();
    PaletteStyleKind::iter()
        .filter_map(|kind| {
            let name = kind.to_string();
            if name.to_lowercase().contains(&current_lower) {
                let candidate = CompletionCandidate::new(format!("{}=NO_SUFFIX", name))
                    .help(kind.get_message().map(clap::builder::StyledStr::from));
                Some(candidate)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_possible_style_name_completions_empty_yields_all() {
        let values: Vec<String> = possible_style_name_completions(std::ffi::OsStr::new(""))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect();
        assert!(values.contains(&"recognised-command=NO_SUFFIX".to_string()));
        assert!(values.contains(&"inline-suggestion=NO_SUFFIX".to_string()));
        assert!(values.contains(&"bash-reserved=NO_SUFFIX".to_string()));
        assert_eq!(values.len(), PaletteStyleKind::iter().count());
    }

    #[test]
    fn test_possible_style_name_completions_partial_filters() {
        let values: Vec<String> = possible_style_name_completions(std::ffi::OsStr::new("inline"))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect();
        assert!(values.contains(&"inline-suggestion=NO_SUFFIX".to_string()));
        assert!(!values.contains(&"recognised-command=NO_SUFFIX".to_string()));
    }

    #[test]
    fn test_possible_style_name_completions_after_equals_returns_empty() {
        let values =
            possible_style_name_completions(std::ffi::OsStr::new("inline-suggestion=bold"));
        assert!(values.is_empty());
    }

    #[test]
    fn test_possible_style_name_completions_have_help() {
        let candidates = possible_style_name_completions(std::ffi::OsStr::new("recognised"));
        assert!(!candidates.is_empty());
        for c in &candidates {
            assert!(c.get_help().is_some());
        }
    }

    #[test]
    fn test_terminal_colors_detects_dark_and_light_themes() {
        // Catppuccin Mocha (dark)
        let mocha = TerminalColors {
            fg: Some((205, 214, 244)),
            bg: Some((30, 30, 46)),
        };
        assert_eq!(mocha.detected_theme(), Some(ColourTheme::Dark));
        assert_eq!(mocha.popup_selected_bg(), Some(Color::Rgb(55, 56, 74)));

        // Solarized Light
        let solarized_light = TerminalColors {
            fg: Some((101, 123, 131)),
            bg: Some((253, 246, 227)),
        };
        assert_eq!(solarized_light.detected_theme(), Some(ColourTheme::Light));
        assert_eq!(
            solarized_light.popup_selected_bg(),
            Some(Color::Rgb(238, 234, 217))
        );

        // Unknown terminal colours
        assert_eq!(TerminalColors::default().detected_theme(), None);
        assert_eq!(TerminalColors::default().popup_selected_bg(), None);
    }

    #[test]
    fn test_palette_apply_theme_preserving_keeps_customized_slots() {
        let mut palette = Palette::dark();
        let custom = Style::default().fg(Color::Magenta).bold();
        palette.set(PaletteStyleKind::PopupIndicator, custom);
        let preserved = HashSet::from([PaletteStyleKind::PopupIndicator]);

        palette.apply_theme_preserving(ColourTheme::Light, &preserved);
        assert_eq!(palette.popup_indicator(), custom);
        assert_eq!(palette.popup_rail(), Palette::light().popup_rail());
    }
}
