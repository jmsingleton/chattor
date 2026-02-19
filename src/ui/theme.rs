use ratatui::style::Color;

/// All colors used across the UI, grouped by component.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    pub name: String,

    // Global
    pub bg: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub accent: Color,
    pub border: Color,
    pub border_focused: Color,

    // Header
    pub header_fg: Color,
    pub header_accent: Color,

    // Sidebar
    pub sidebar_selected_fg: Color,
    pub sidebar_unread: Color,
    pub sidebar_status_online: Color,
    pub sidebar_channel_header: Color,

    // Conversation
    pub msg_own_sender: Color,
    pub msg_peer_sender: Color,
    pub msg_timestamp: Color,
    pub msg_status_sent: Color,
    pub msg_status_delivered: Color,
    pub msg_status_read: Color,
    pub msg_status_failed: Color,
    pub msg_ephemeral: Color,

    // Input
    pub input_fg: Color,
    pub input_placeholder: Color,

    // Modals
    pub modal_border: Color,
    pub modal_title: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,

    // Channel feed
    pub channel_border: Color,
    pub channel_read_count: Color,
}

/// Parse a hex color string like "#00d4ff" into a ratatui Color::Rgb.
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Shorthand: parse hex or panic (for preset definitions only).
fn h(hex: &str) -> Color {
    parse_hex_color(hex).unwrap_or(Color::White)
}

impl Theme {
    /// Load a preset theme by name. Falls back to "dark" for unknown names.
    pub fn preset(name: &str) -> Self {
        match name {
            "dark" => Self::dark(),
            "light" => Self::light(),
            "cyberpunk" => Self::cyberpunk(),
            "minimal" => Self::minimal(),
            "rose-pine" => Self::rose_pine(),
            "rose-pine-moon" => Self::rose_pine_moon(),
            "rose-pine-dawn" => Self::rose_pine_dawn(),
            _ => Self::dark(),
        }
    }

    /// List all available preset names.
    #[allow(dead_code)]
    pub fn preset_names() -> &'static [&'static str] {
        &["dark", "light", "cyberpunk", "minimal", "rose-pine", "rose-pine-moon", "rose-pine-dawn"]
    }

    fn dark() -> Self {
        Theme {
            name: "dark".into(),
            bg: h("#0a0e27"),
            fg: h("#e0e0e0"),
            fg_dim: h("#666666"),
            accent: h("#00d4ff"),
            border: h("#333355"),
            border_focused: h("#00d4ff"),
            header_fg: h("#00d4ff"),
            header_accent: h("#00d4ff"),
            sidebar_selected_fg: h("#ffffff"),
            sidebar_unread: h("#ffcc00"),
            sidebar_status_online: h("#00ff88"),
            sidebar_channel_header: h("#00d4ff"),
            msg_own_sender: h("#00d4ff"),
            msg_peer_sender: h("#00ff88"),
            msg_timestamp: h("#666666"),
            msg_status_sent: h("#666666"),
            msg_status_delivered: h("#666666"),
            msg_status_read: h("#00ff88"),
            msg_status_failed: h("#ff4444"),
            msg_ephemeral: h("#ffcc00"),
            input_fg: h("#ffffff"),
            input_placeholder: h("#666666"),
            modal_border: h("#00d4ff"),
            modal_title: h("#00d4ff"),
            error: h("#ff4444"),
            warning: h("#ffcc00"),
            success: h("#00ff88"),
            channel_border: h("#cc44ff"),
            channel_read_count: h("#666666"),
        }
    }

    fn light() -> Self {
        Theme {
            name: "light".into(),
            bg: h("#f5f5f5"),
            fg: h("#1a1a1a"),
            fg_dim: h("#888888"),
            accent: h("#0066cc"),
            border: h("#cccccc"),
            border_focused: h("#0066cc"),
            header_fg: h("#0066cc"),
            header_accent: h("#0066cc"),
            sidebar_selected_fg: h("#000000"),
            sidebar_unread: h("#cc6600"),
            sidebar_status_online: h("#008844"),
            sidebar_channel_header: h("#0066cc"),
            msg_own_sender: h("#0066cc"),
            msg_peer_sender: h("#008844"),
            msg_timestamp: h("#888888"),
            msg_status_sent: h("#888888"),
            msg_status_delivered: h("#888888"),
            msg_status_read: h("#008844"),
            msg_status_failed: h("#cc0000"),
            msg_ephemeral: h("#cc6600"),
            input_fg: h("#1a1a1a"),
            input_placeholder: h("#888888"),
            modal_border: h("#0066cc"),
            modal_title: h("#0066cc"),
            error: h("#cc0000"),
            warning: h("#cc6600"),
            success: h("#008844"),
            channel_border: h("#8844aa"),
            channel_read_count: h("#888888"),
        }
    }

    fn cyberpunk() -> Self {
        Theme {
            name: "cyberpunk".into(),
            bg: h("#000000"),
            fg: h("#00ff41"),
            fg_dim: h("#005500"),
            accent: h("#00ff41"),
            border: h("#003300"),
            border_focused: h("#00ff41"),
            header_fg: h("#00ff41"),
            header_accent: h("#00ff41"),
            sidebar_selected_fg: h("#00ff41"),
            sidebar_unread: h("#ffff00"),
            sidebar_status_online: h("#00ff41"),
            sidebar_channel_header: h("#00ff41"),
            msg_own_sender: h("#00ff41"),
            msg_peer_sender: h("#00cc33"),
            msg_timestamp: h("#005500"),
            msg_status_sent: h("#005500"),
            msg_status_delivered: h("#005500"),
            msg_status_read: h("#00ff41"),
            msg_status_failed: h("#ff0000"),
            msg_ephemeral: h("#ffff00"),
            input_fg: h("#00ff41"),
            input_placeholder: h("#005500"),
            modal_border: h("#00ff41"),
            modal_title: h("#00ff41"),
            error: h("#ff0000"),
            warning: h("#ffff00"),
            success: h("#00ff41"),
            channel_border: h("#00ff41"),
            channel_read_count: h("#005500"),
        }
    }

    fn minimal() -> Self {
        Theme {
            name: "minimal".into(),
            bg: h("#000000"),
            fg: h("#cccccc"),
            fg_dim: h("#555555"),
            accent: h("#ffffff"),
            border: h("#333333"),
            border_focused: h("#ffffff"),
            header_fg: h("#ffffff"),
            header_accent: h("#ffffff"),
            sidebar_selected_fg: h("#ffffff"),
            sidebar_unread: h("#ffffff"),
            sidebar_status_online: h("#aaaaaa"),
            sidebar_channel_header: h("#ffffff"),
            msg_own_sender: h("#ffffff"),
            msg_peer_sender: h("#aaaaaa"),
            msg_timestamp: h("#555555"),
            msg_status_sent: h("#555555"),
            msg_status_delivered: h("#888888"),
            msg_status_read: h("#ffffff"),
            msg_status_failed: h("#888888"),
            msg_ephemeral: h("#aaaaaa"),
            input_fg: h("#cccccc"),
            input_placeholder: h("#555555"),
            modal_border: h("#ffffff"),
            modal_title: h("#ffffff"),
            error: h("#ffffff"),
            warning: h("#aaaaaa"),
            success: h("#ffffff"),
            channel_border: h("#ffffff"),
            channel_read_count: h("#555555"),
        }
    }

    fn rose_pine() -> Self {
        Theme {
            name: "rose-pine".into(),
            bg: h("#191724"),
            fg: h("#e0def4"),
            fg_dim: h("#6e6a86"),
            accent: h("#ebbcba"),
            border: h("#26233a"),
            border_focused: h("#ebbcba"),
            header_fg: h("#e0def4"),
            header_accent: h("#ebbcba"),
            sidebar_selected_fg: h("#e0def4"),
            sidebar_unread: h("#f6c177"),
            sidebar_status_online: h("#9ccfd8"),
            sidebar_channel_header: h("#c4a7e7"),
            msg_own_sender: h("#ebbcba"),
            msg_peer_sender: h("#9ccfd8"),
            msg_timestamp: h("#6e6a86"),
            msg_status_sent: h("#6e6a86"),
            msg_status_delivered: h("#908caa"),
            msg_status_read: h("#9ccfd8"),
            msg_status_failed: h("#eb6f92"),
            msg_ephemeral: h("#f6c177"),
            input_fg: h("#e0def4"),
            input_placeholder: h("#6e6a86"),
            modal_border: h("#ebbcba"),
            modal_title: h("#ebbcba"),
            error: h("#eb6f92"),
            warning: h("#f6c177"),
            success: h("#9ccfd8"),
            channel_border: h("#c4a7e7"),
            channel_read_count: h("#6e6a86"),
        }
    }

    fn rose_pine_moon() -> Self {
        Theme {
            name: "rose-pine-moon".into(),
            bg: h("#232136"),
            fg: h("#e0def4"),
            fg_dim: h("#6e6a86"),
            accent: h("#ea9a97"),
            border: h("#393552"),
            border_focused: h("#ea9a97"),
            header_fg: h("#e0def4"),
            header_accent: h("#ea9a97"),
            sidebar_selected_fg: h("#e0def4"),
            sidebar_unread: h("#f6c177"),
            sidebar_status_online: h("#9ccfd8"),
            sidebar_channel_header: h("#c4a7e7"),
            msg_own_sender: h("#ea9a97"),
            msg_peer_sender: h("#9ccfd8"),
            msg_timestamp: h("#6e6a86"),
            msg_status_sent: h("#6e6a86"),
            msg_status_delivered: h("#908caa"),
            msg_status_read: h("#9ccfd8"),
            msg_status_failed: h("#eb6f92"),
            msg_ephemeral: h("#f6c177"),
            input_fg: h("#e0def4"),
            input_placeholder: h("#6e6a86"),
            modal_border: h("#ea9a97"),
            modal_title: h("#ea9a97"),
            error: h("#eb6f92"),
            warning: h("#f6c177"),
            success: h("#9ccfd8"),
            channel_border: h("#c4a7e7"),
            channel_read_count: h("#6e6a86"),
        }
    }

    fn rose_pine_dawn() -> Self {
        Theme {
            name: "rose-pine-dawn".into(),
            bg: h("#faf4ed"),
            fg: h("#575279"),
            fg_dim: h("#9893a5"),
            accent: h("#d7827e"),
            border: h("#f2e9e1"),
            border_focused: h("#d7827e"),
            header_fg: h("#575279"),
            header_accent: h("#d7827e"),
            sidebar_selected_fg: h("#575279"),
            sidebar_unread: h("#ea9d34"),
            sidebar_status_online: h("#56949f"),
            sidebar_channel_header: h("#907aa9"),
            msg_own_sender: h("#d7827e"),
            msg_peer_sender: h("#56949f"),
            msg_timestamp: h("#9893a5"),
            msg_status_sent: h("#9893a5"),
            msg_status_delivered: h("#797593"),
            msg_status_read: h("#56949f"),
            msg_status_failed: h("#b4637a"),
            msg_ephemeral: h("#ea9d34"),
            input_fg: h("#575279"),
            input_placeholder: h("#9893a5"),
            modal_border: h("#d7827e"),
            modal_title: h("#d7827e"),
            error: h("#b4637a"),
            warning: h("#ea9d34"),
            success: h("#56949f"),
            channel_border: h("#907aa9"),
            channel_read_count: h("#9893a5"),
        }
    }

    /// Apply TOML color overrides on top of the current theme.
    /// Each field is optional — only specified overrides are applied.
    pub fn apply_overrides(&mut self, overrides: &ThemeOverrides) {
        if let Some(c) = overrides.bg.as_deref().and_then(parse_hex_color) { self.bg = c; }
        if let Some(c) = overrides.fg.as_deref().and_then(parse_hex_color) { self.fg = c; }
        if let Some(c) = overrides.fg_dim.as_deref().and_then(parse_hex_color) { self.fg_dim = c; }
        if let Some(c) = overrides.accent.as_deref().and_then(parse_hex_color) { self.accent = c; }
        if let Some(c) = overrides.border.as_deref().and_then(parse_hex_color) { self.border = c; }
        if let Some(c) = overrides.border_focused.as_deref().and_then(parse_hex_color) { self.border_focused = c; }
        if let Some(c) = overrides.error.as_deref().and_then(parse_hex_color) { self.error = c; }
        if let Some(c) = overrides.warning.as_deref().and_then(parse_hex_color) { self.warning = c; }
        if let Some(c) = overrides.success.as_deref().and_then(parse_hex_color) { self.success = c; }
    }
}

/// Subset of theme colors that can be overridden via TOML config.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ThemeOverrides {
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub fg_dim: Option<String>,
    pub accent: Option<String>,
    pub border: Option<String>,
    pub border_focused: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub success: Option<String>,
}

/// TOML config file structure for `~/.config/chattor/theme.toml`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ThemeConfig {
    pub preset: Option<String>,
    #[serde(default)]
    pub colors: ThemeOverrides,
}

/// Load theme from CLI flag, config file, or default.
///
/// Priority: CLI flag > config file > "dark" default.
pub fn load_theme(cli_preset: Option<&str>, config_path: &std::path::Path) -> Theme {
    if let Some(name) = cli_preset {
        return Theme::preset(name);
    }

    if let Ok(content) = std::fs::read_to_string(config_path) {
        if let Ok(config) = toml::from_str::<ThemeConfig>(&content) {
            let mut theme = Theme::preset(config.preset.as_deref().unwrap_or("dark"));
            theme.apply_overrides(&config.colors);
            return theme;
        }
    }

    Theme::preset("dark")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#00d4ff"), Some(Color::Rgb(0, 212, 255)));
        assert_eq!(parse_hex_color("#ffffff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_hex_color("#000000"), Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn test_parse_hex_color_invalid() {
        assert_eq!(parse_hex_color("not-a-color"), None);
        assert_eq!(parse_hex_color("#gggggg"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn test_dark_preset_has_expected_colors() {
        let theme = Theme::preset("dark");
        assert_eq!(theme.name, "dark");
        assert_eq!(theme.bg, Color::Rgb(10, 14, 39));
        assert_eq!(theme.accent, Color::Rgb(0, 212, 255));
    }

    #[test]
    fn test_all_presets_exist() {
        for name in Theme::preset_names() {
            let theme = Theme::preset(name);
            assert_eq!(&theme.name, name);
        }
    }

    #[test]
    fn test_unknown_preset_falls_back_to_dark() {
        let theme = Theme::preset("nonexistent");
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn test_apply_overrides() {
        let mut theme = Theme::preset("dark");
        let overrides = ThemeOverrides {
            accent: Some("#ff0000".into()),
            ..Default::default()
        };
        theme.apply_overrides(&overrides);
        assert_eq!(theme.accent, Color::Rgb(255, 0, 0));
        // Other fields unchanged
        assert_eq!(theme.bg, Color::Rgb(10, 14, 39));
    }

    #[test]
    fn test_load_theme_cli_takes_priority() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "preset = \"light\"").unwrap();
        let theme = load_theme(Some("cyberpunk"), tmp.path());
        assert_eq!(theme.name, "cyberpunk");
    }

    #[test]
    fn test_load_theme_from_config_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "preset = \"minimal\"\n\n[colors]\naccent = \"#abcdef\"").unwrap();
        let theme = load_theme(None, tmp.path());
        assert_eq!(theme.name, "minimal");
        assert_eq!(theme.accent, Color::Rgb(171, 205, 239));
    }

    #[test]
    fn test_load_theme_default_dark() {
        let theme = load_theme(None, std::path::Path::new("/nonexistent/path"));
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn test_theme_config_deserialization() {
        let toml_str = r##"
preset = "rose-pine"

[colors]
accent = "#00ffaa"
bg = "#111111"
"##;
        let config: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.preset.as_deref(), Some("rose-pine"));
        assert_eq!(config.colors.accent.as_deref(), Some("#00ffaa"));
        assert_eq!(config.colors.bg.as_deref(), Some("#111111"));
        assert!(config.colors.fg.is_none());
    }
}
