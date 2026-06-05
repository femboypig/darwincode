use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SingleColor {
    Hex(String),   // e.g. "#2E3440"
    Ansi(u8),      // e.g. 3
    Named(String), // e.g. "none", "nord0", etc.
}

impl<'de> serde::Deserialize<'de> for SingleColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SingleColorVisitor;

        impl<'de> serde::de::Visitor<'de> for SingleColorVisitor {
            type Value = SingleColor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a hex color string starting with '#', a color name string, or an ANSI u8 integer")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.starts_with('#') {
                    Ok(SingleColor::Hex(value.to_string()))
                } else {
                    Ok(SingleColor::Named(value.to_string()))
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value <= 255 {
                    Ok(SingleColor::Ansi(value as u8))
                } else {
                    Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(value),
                        &"an ANSI color code between 0 and 255",
                    ))
                }
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if (0..=255).contains(&value) {
                    Ok(SingleColor::Ansi(value as u8))
                } else {
                    Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Signed(value),
                        &"an ANSI color code between 0 and 255",
                    ))
                }
            }
        }

        deserializer.deserialize_any(SingleColorVisitor)
    }
}

impl serde::Serialize for SingleColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            SingleColor::Hex(hex) => serializer.serialize_str(hex),
            SingleColor::Ansi(code) => serializer.serialize_u8(*code),
            SingleColor::Named(name) => serializer.serialize_str(name),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum ColorValue {
    Single(SingleColor),
    DarkLight {
        dark: SingleColor,
        light: SingleColor,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    pub primary: ColorValue,
    pub secondary: ColorValue,
    pub accent: ColorValue,
    pub error: ColorValue,
    pub warning: ColorValue,
    pub success: ColorValue,
    pub info: ColorValue,
    pub text: ColorValue,
    pub text_muted: ColorValue,
    pub background: ColorValue,
    #[serde(default)]
    pub background_panel: Option<ColorValue>,
    #[serde(default)]
    pub background_element: Option<ColorValue>,
    #[serde(default)]
    pub border: Option<ColorValue>,
    #[serde(default)]
    pub border_active: Option<ColorValue>,
    #[serde(default)]
    pub border_subtle: Option<ColorValue>,

    // Git Diff
    #[serde(default)]
    pub diff_added: Option<ColorValue>,
    #[serde(default)]
    pub diff_removed: Option<ColorValue>,
    #[serde(default)]
    pub diff_context: Option<ColorValue>,
    #[serde(default)]
    pub diff_hunk_header: Option<ColorValue>,
    #[serde(default)]
    pub diff_highlight_added: Option<ColorValue>,
    #[serde(default)]
    pub diff_highlight_removed: Option<ColorValue>,
    #[serde(default)]
    pub diff_added_bg: Option<ColorValue>,
    #[serde(default)]
    pub diff_removed_bg: Option<ColorValue>,
    #[serde(default)]
    pub diff_context_bg: Option<ColorValue>,
    #[serde(default)]
    pub diff_line_number: Option<ColorValue>,
    #[serde(default)]
    pub diff_added_line_number_bg: Option<ColorValue>,
    #[serde(default)]
    pub diff_removed_line_number_bg: Option<ColorValue>,

    // Markdown
    #[serde(default)]
    pub markdown_text: Option<ColorValue>,
    #[serde(default)]
    pub markdown_heading: Option<ColorValue>,
    #[serde(default)]
    pub markdown_link: Option<ColorValue>,
    #[serde(default)]
    pub markdown_link_text: Option<ColorValue>,
    #[serde(default)]
    pub markdown_code: Option<ColorValue>,
    #[serde(default)]
    pub markdown_block_quote: Option<ColorValue>,
    #[serde(default)]
    pub markdown_emph: Option<ColorValue>,
    #[serde(default)]
    pub markdown_strong: Option<ColorValue>,
    #[serde(default)]
    pub markdown_horizontal_rule: Option<ColorValue>,
    #[serde(default)]
    pub markdown_list_item: Option<ColorValue>,
    #[serde(default)]
    pub markdown_list_enumeration: Option<ColorValue>,
    #[serde(default)]
    pub markdown_image: Option<ColorValue>,
    #[serde(default)]
    pub markdown_image_text: Option<ColorValue>,
    #[serde(default)]
    pub markdown_code_block: Option<ColorValue>,

    // Syntax
    #[serde(default)]
    pub syntax_comment: Option<ColorValue>,
    #[serde(default)]
    pub syntax_keyword: Option<ColorValue>,
    #[serde(default)]
    pub syntax_function: Option<ColorValue>,
    #[serde(default)]
    pub syntax_variable: Option<ColorValue>,
    #[serde(default)]
    pub syntax_string: Option<ColorValue>,
    #[serde(default)]
    pub syntax_number: Option<ColorValue>,
    #[serde(default)]
    pub syntax_type: Option<ColorValue>,
    #[serde(default)]
    pub syntax_operator: Option<ColorValue>,
    #[serde(default)]
    pub syntax_punctuation: Option<ColorValue>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ThemeConfig {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub defs: HashMap<String, ColorValue>,
    pub theme: ThemeColors,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ActiveTheme {
    pub is_light: bool,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
    pub text: Color,
    pub text_muted: Color,
    pub background: Option<Color>,
    pub background_panel: Option<Color>,
    pub background_element: Option<Color>,
    pub border: Color,
    pub border_active: Color,
    pub border_subtle: Color,

    // Git Diff
    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_context: Color,
    pub diff_hunk_header: Color,
    pub diff_highlight_added: Color,
    pub diff_highlight_removed: Color,
    pub diff_added_bg: Option<Color>,
    pub diff_removed_bg: Option<Color>,
    pub diff_context_bg: Option<Color>,
    pub diff_line_number: Color,
    pub diff_added_line_number_bg: Option<Color>,
    pub diff_removed_line_number_bg: Option<Color>,

    // Markdown
    pub markdown_text: Color,
    pub markdown_heading: Color,
    pub markdown_link: Color,
    pub markdown_link_text: Color,
    pub markdown_code: Color,
    pub markdown_block_quote: Color,
    pub markdown_emph: Color,
    pub markdown_strong: Color,
    pub markdown_horizontal_rule: Color,
    pub markdown_list_item: Color,
    pub markdown_list_enumeration: Color,
    pub markdown_image: Color,
    pub markdown_image_text: Color,
    pub markdown_code_block: Option<Color>,

    // Syntax
    pub syntax_comment: Color,
    pub syntax_keyword: Color,
    pub syntax_function: Color,
    pub syntax_variable: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_type: Color,
    pub syntax_operator: Color,
    pub syntax_punctuation: Color,
}

impl Default for ActiveTheme {
    fn default() -> Self {
        // Fallback dark theme
        Self {
            is_light: false,
            primary: Color::Rgb(255, 255, 255),
            secondary: Color::Rgb(110, 110, 110),
            accent: Color::Rgb(134, 194, 172),
            error: Color::Rgb(220, 60, 60),
            warning: Color::Rgb(220, 180, 60),
            success: Color::Rgb(60, 200, 60),
            info: Color::Rgb(60, 180, 220),
            text: Color::Rgb(255, 255, 255),
            text_muted: Color::Rgb(160, 160, 160),
            background: Some(Color::Rgb(15, 15, 15)),
            background_panel: Some(Color::Rgb(24, 24, 24)),
            background_element: Some(Color::Rgb(24, 24, 24)),
            border: Color::Rgb(60, 60, 60),
            border_active: Color::Rgb(134, 194, 172),
            border_subtle: Color::Rgb(40, 40, 40),

            diff_added: Color::Rgb(60, 200, 60),
            diff_removed: Color::Rgb(220, 60, 60),
            diff_context: Color::Rgb(160, 160, 160),
            diff_hunk_header: Color::Rgb(60, 180, 220),
            diff_highlight_added: Color::Rgb(60, 200, 60),
            diff_highlight_removed: Color::Rgb(220, 60, 60),
            diff_added_bg: None,
            diff_removed_bg: None,
            diff_context_bg: None,
            diff_line_number: Color::Rgb(110, 110, 110),
            diff_added_line_number_bg: None,
            diff_removed_line_number_bg: None,

            markdown_text: Color::Rgb(255, 255, 255),
            markdown_heading: Color::Rgb(255, 255, 255),
            markdown_link: Color::Rgb(134, 194, 172),
            markdown_link_text: Color::Rgb(134, 194, 172),
            markdown_code: Color::Rgb(134, 194, 172),
            markdown_block_quote: Color::Rgb(160, 160, 160),
            markdown_emph: Color::Rgb(134, 194, 172),
            markdown_strong: Color::Rgb(255, 255, 255),
            markdown_horizontal_rule: Color::Rgb(60, 60, 60),
            markdown_list_item: Color::Rgb(134, 194, 172),
            markdown_list_enumeration: Color::Rgb(134, 194, 172),
            markdown_image: Color::Rgb(134, 194, 172),
            markdown_image_text: Color::Rgb(134, 194, 172),
            markdown_code_block: Some(Color::Rgb(24, 24, 24)),

            syntax_comment: Color::DarkGray,
            syntax_keyword: Color::Magenta,
            syntax_function: Color::Blue,
            syntax_variable: Color::Cyan,
            syntax_string: Color::Green,
            syntax_number: Color::Yellow,
            syntax_type: Color::Blue,
            syntax_operator: Color::Cyan,
            syntax_punctuation: Color::Cyan,
        }
    }
}

impl ActiveTheme {
    pub fn light_default() -> Self {
        Self {
            is_light: true,
            primary: Color::Rgb(190, 60, 100),
            secondary: Color::Rgb(140, 140, 140),
            accent: Color::Rgb(190, 60, 100),
            error: Color::Rgb(200, 40, 40),
            warning: Color::Rgb(180, 140, 40),
            success: Color::Rgb(40, 160, 40),
            info: Color::Rgb(40, 120, 180),
            text: Color::Rgb(30, 30, 30),
            text_muted: Color::Rgb(140, 140, 140),
            background: Some(Color::Rgb(248, 248, 248)),
            background_panel: Some(Color::Rgb(240, 240, 240)),
            background_element: Some(Color::Rgb(240, 240, 240)),
            border: Color::Rgb(200, 200, 205),
            border_active: Color::Rgb(190, 60, 100),
            border_subtle: Color::Rgb(220, 220, 225),

            diff_added: Color::Rgb(40, 160, 40),
            diff_removed: Color::Rgb(200, 40, 40),
            diff_context: Color::Rgb(140, 140, 140),
            diff_hunk_header: Color::Rgb(40, 120, 180),
            diff_highlight_added: Color::Rgb(40, 160, 40),
            diff_highlight_removed: Color::Rgb(200, 40, 40),
            diff_added_bg: None,
            diff_removed_bg: None,
            diff_context_bg: None,
            diff_line_number: Color::Rgb(150, 150, 150),
            diff_added_line_number_bg: None,
            diff_removed_line_number_bg: None,

            markdown_text: Color::Rgb(30, 30, 30),
            markdown_heading: Color::Rgb(30, 30, 30),
            markdown_link: Color::Rgb(190, 60, 100),
            markdown_link_text: Color::Rgb(190, 60, 100),
            markdown_code: Color::Rgb(190, 60, 100),
            markdown_block_quote: Color::Rgb(140, 140, 140),
            markdown_emph: Color::Rgb(190, 60, 100),
            markdown_strong: Color::Rgb(30, 30, 30),
            markdown_horizontal_rule: Color::Rgb(200, 200, 205),
            markdown_list_item: Color::Rgb(190, 60, 100),
            markdown_list_enumeration: Color::Rgb(190, 60, 100),
            markdown_image: Color::Rgb(190, 60, 100),
            markdown_image_text: Color::Rgb(190, 60, 100),
            markdown_code_block: Some(Color::Rgb(240, 240, 240)),

            syntax_comment: Color::DarkGray,
            syntax_keyword: Color::Magenta,
            syntax_function: Color::Blue,
            syntax_variable: Color::Cyan,
            syntax_string: Color::Green,
            syntax_number: Color::Yellow,
            syntax_type: Color::Blue,
            syntax_operator: Color::Cyan,
            syntax_punctuation: Color::Cyan,
        }
    }
}

pub fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

fn resolve_single_color(
    sc: &SingleColor,
    mode: &ThemeMode,
    defs: &HashMap<String, ColorValue>,
    depth: usize,
) -> Option<Color> {
    if depth > 5 {
        return None; // Prevent infinite recursion
    }
    match sc {
        SingleColor::Ansi(code) => Some(Color::Indexed(*code)),
        SingleColor::Hex(hex_str) => parse_hex(hex_str),
        SingleColor::Named(name) => {
            if name == "none" {
                None
            } else if let Some(cv) = defs.get(name) {
                resolve_color_value(cv, mode, defs, depth + 1)
            } else {
                None
            }
        }
    }
}

fn resolve_color_value(
    cv: &ColorValue,
    mode: &ThemeMode,
    defs: &HashMap<String, ColorValue>,
    depth: usize,
) -> Option<Color> {
    match cv {
        ColorValue::Single(sc) => resolve_single_color(sc, mode, defs, depth),
        ColorValue::DarkLight { dark, light } => match mode {
            ThemeMode::Dark => resolve_single_color(dark, mode, defs, depth),
            ThemeMode::Light => resolve_single_color(light, mode, defs, depth),
        },
    }
}

impl ThemeConfig {
    pub fn resolve(&self, mode: &ThemeMode) -> ActiveTheme {
        let defs = &self.defs;
        let resolve = |cv: &ColorValue| resolve_color_value(cv, mode, defs, 0);
        let resolve_opt = |opt: &Option<ColorValue>| opt.as_ref().and_then(resolve);

        let base_default = match mode {
            ThemeMode::Dark => ActiveTheme::default(),
            ThemeMode::Light => ActiveTheme::light_default(),
        };

        let primary = resolve(&self.theme.primary).unwrap_or(base_default.primary);
        let secondary = resolve(&self.theme.secondary).unwrap_or(base_default.secondary);
        let accent = resolve(&self.theme.accent).unwrap_or(base_default.accent);
        let error = resolve(&self.theme.error).unwrap_or(base_default.error);
        let warning = resolve(&self.theme.warning).unwrap_or(base_default.warning);
        let success = resolve(&self.theme.success).unwrap_or(base_default.success);
        let info = resolve(&self.theme.info).unwrap_or(base_default.info);
        let text = resolve(&self.theme.text).unwrap_or(base_default.text);
        let text_muted = resolve(&self.theme.text_muted).unwrap_or(base_default.text_muted);
        let background = resolve_color_value(&self.theme.background, mode, defs, 0);

        let is_light = match mode {
            ThemeMode::Dark => false,
            ThemeMode::Light => true,
        };

        ActiveTheme {
            is_light,
            primary,
            secondary,
            accent,
            error,
            warning,
            success,
            info,
            text,
            text_muted,
            background,
            background_panel: resolve_opt(&self.theme.background_panel).or(background),
            background_element: resolve_opt(&self.theme.background_element).or(background),
            border: resolve_opt(&self.theme.border).unwrap_or(text_muted),
            border_active: resolve_opt(&self.theme.border_active).unwrap_or(accent),
            border_subtle: resolve_opt(&self.theme.border_subtle)
                .unwrap_or(base_default.border_subtle),

            // Git Diff
            diff_added: resolve_opt(&self.theme.diff_added).unwrap_or(success),
            diff_removed: resolve_opt(&self.theme.diff_removed).unwrap_or(error),
            diff_context: resolve_opt(&self.theme.diff_context).unwrap_or(text_muted),
            diff_hunk_header: resolve_opt(&self.theme.diff_hunk_header).unwrap_or(info),
            diff_highlight_added: resolve_opt(&self.theme.diff_highlight_added).unwrap_or(success),
            diff_highlight_removed: resolve_opt(&self.theme.diff_highlight_removed)
                .unwrap_or(error),
            diff_added_bg: resolve_opt(&self.theme.diff_added_bg),
            diff_removed_bg: resolve_opt(&self.theme.diff_removed_bg),
            diff_context_bg: resolve_opt(&self.theme.diff_context_bg),
            diff_line_number: resolve_opt(&self.theme.diff_line_number).unwrap_or(text_muted),
            diff_added_line_number_bg: resolve_opt(&self.theme.diff_added_line_number_bg),
            diff_removed_line_number_bg: resolve_opt(&self.theme.diff_removed_line_number_bg),

            // Markdown
            markdown_text: resolve_opt(&self.theme.markdown_text).unwrap_or(text),
            markdown_heading: resolve_opt(&self.theme.markdown_heading).unwrap_or(primary),
            markdown_link: resolve_opt(&self.theme.markdown_link).unwrap_or(accent),
            markdown_link_text: resolve_opt(&self.theme.markdown_link_text).unwrap_or(accent),
            markdown_code: resolve_opt(&self.theme.markdown_code).unwrap_or(accent),
            markdown_block_quote: resolve_opt(&self.theme.markdown_block_quote)
                .unwrap_or(text_muted),
            markdown_emph: resolve_opt(&self.theme.markdown_emph).unwrap_or(accent),
            markdown_strong: resolve_opt(&self.theme.markdown_strong).unwrap_or(primary),
            markdown_horizontal_rule: resolve_opt(&self.theme.markdown_horizontal_rule)
                .unwrap_or(base_default.markdown_horizontal_rule),
            markdown_list_item: resolve_opt(&self.theme.markdown_list_item).unwrap_or(accent),
            markdown_list_enumeration: resolve_opt(&self.theme.markdown_list_enumeration)
                .unwrap_or(accent),
            markdown_image: resolve_opt(&self.theme.markdown_image).unwrap_or(accent),
            markdown_image_text: resolve_opt(&self.theme.markdown_image_text).unwrap_or(accent),
            markdown_code_block: resolve_opt(&self.theme.markdown_code_block).or(background),

            // Syntax
            syntax_comment: resolve_opt(&self.theme.syntax_comment)
                .unwrap_or(base_default.syntax_comment),
            syntax_keyword: resolve_opt(&self.theme.syntax_keyword)
                .unwrap_or(base_default.syntax_keyword),
            syntax_function: resolve_opt(&self.theme.syntax_function)
                .unwrap_or(base_default.syntax_function),
            syntax_variable: resolve_opt(&self.theme.syntax_variable)
                .unwrap_or(base_default.syntax_variable),
            syntax_string: resolve_opt(&self.theme.syntax_string)
                .unwrap_or(base_default.syntax_string),
            syntax_number: resolve_opt(&self.theme.syntax_number)
                .unwrap_or(base_default.syntax_number),
            syntax_type: resolve_opt(&self.theme.syntax_type).unwrap_or(base_default.syntax_type),
            syntax_operator: resolve_opt(&self.theme.syntax_operator)
                .unwrap_or(base_default.syntax_operator),
            syntax_punctuation: resolve_opt(&self.theme.syntax_punctuation)
                .unwrap_or(base_default.syntax_punctuation),
        }
    }
}

pub fn find_project_root() -> Option<PathBuf> {
    let mut cwd = std::env::current_dir().ok()?;
    loop {
        if cwd.join(".git").exists() {
            return Some(cwd);
        }
        if let Some(parent) = cwd.parent() {
            cwd = parent.to_path_buf();
        } else {
            break;
        }
    }
    None
}

pub fn custom_themes() -> &'static HashMap<String, ThemeConfig> {
    static CUSTOM_THEMES: std::sync::OnceLock<HashMap<String, ThemeConfig>> =
        std::sync::OnceLock::new();
    CUSTOM_THEMES.get_or_init(|| {
        let mut themes = HashMap::new();

        // Load built-in themes
        // (To avoid include_str errors or parsing failure, we define built-in themes directly in JSON)
        let builtins = vec![
            ("tokyonight", include_str!("themes/tokyonight.json")),
            ("nord", include_str!("themes/nord.json")),
            ("gruvbox", include_str!("themes/gruvbox.json")),
            ("ayu", include_str!("themes/ayu.json")),
            ("everforest", include_str!("themes/everforest.json")),
        ];

        for (name, content) in builtins {
            if let Ok(tc) = serde_json::from_str::<ThemeConfig>(content) {
                themes.insert(name.to_string(), tc);
            }
        }

        // Scan theme directories
        // 1. User config: ~/.config/darwincode/themes/*.json
        if let Some(mut user_dir) = dirs::config_dir() {
            user_dir.push("darwincode");
            user_dir.push("themes");
            load_themes_from_dir(&user_dir, &mut themes);
        }

        // 2. Project root: <project-root>/.darwincode/themes/*.json
        if let Some(proj_root) = find_project_root() {
            let proj_dir = proj_root.join(".darwincode").join("themes");
            load_themes_from_dir(&proj_dir, &mut themes);
        }

        // 3. Current working directory: ./.darwincode/themes/*.json
        let cwd_dir = PathBuf::from(".").join(".darwincode").join("themes");
        load_themes_from_dir(&cwd_dir, &mut themes);

        themes
    })
}

fn load_themes_from_dir(dir: &std::path::Path, map: &mut HashMap<String, ThemeConfig>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().is_some_and(|ext| ext == "json")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(tc) = serde_json::from_str::<ThemeConfig>(&content)
            {
                map.insert(stem.to_string(), tc);
            }
        }
    }
}

// Module dirs fallback for config directory
mod dirs {
    use std::path::PathBuf;
    pub fn config_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_themes_parse() {
        let builtins = vec![
            ("tokyonight", include_str!("themes/tokyonight.json")),
            ("nord", include_str!("themes/nord.json")),
            ("gruvbox", include_str!("themes/gruvbox.json")),
            ("ayu", include_str!("themes/ayu.json")),
            ("everforest", include_str!("themes/everforest.json")),
        ];

        for (name, content) in builtins {
            let res = serde_json::from_str::<ThemeConfig>(content);
            assert!(
                res.is_ok(),
                "Failed to parse built-in theme '{}': {:?}",
                name,
                res.err()
            );
        }
    }

    #[test]
    fn test_builtin_themes_resolve() {
        let builtins = vec![
            ("tokyonight", include_str!("themes/tokyonight.json")),
            ("nord", include_str!("themes/nord.json")),
            ("gruvbox", include_str!("themes/gruvbox.json")),
            ("ayu", include_str!("themes/ayu.json")),
            ("everforest", include_str!("themes/everforest.json")),
        ];

        for (name, content) in builtins {
            let tc = serde_json::from_str::<ThemeConfig>(content)
                .unwrap_or_else(|e| panic!("Failed to parse builtin '{}': {:?}", name, e));

            // Check resolve in dark mode
            let dark_resolved = tc.resolve(&ThemeMode::Dark);
            if name == "tokyonight" {
                assert_eq!(dark_resolved.primary, Color::Rgb(122, 162, 247));
                assert!(!dark_resolved.is_light);
            }

            // Check resolve in light mode
            let light_resolved = tc.resolve(&ThemeMode::Light);
            if name == "tokyonight" {
                assert_eq!(light_resolved.primary, Color::Rgb(52, 84, 140));
                assert!(light_resolved.is_light);
            }
        }
    }
}
