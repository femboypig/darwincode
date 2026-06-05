use crate::config::Theme;

#[derive(Debug, Default)]
pub struct ThemePickerState {
    pub themes: Vec<Theme>,
    pub selected: usize,
    pub query: String,
}

impl ThemePickerState {
    pub fn new(current_theme: &Theme) -> Self {
        let mut themes = vec![Theme::Auto, Theme::Dark, Theme::Light];

        let mut custom: Vec<String> = crate::tui::theme::custom_themes().keys().cloned().collect();
        custom.sort();

        for name in custom {
            themes.push(Theme::Custom(name));
        }

        let selected = themes
            .iter()
            .position(|theme| theme == current_theme)
            .unwrap_or(0);

        Self {
            themes,
            selected,
            query: String::new(),
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.query.is_empty() {
            return (0..self.themes.len()).collect();
        }
        let q = self.query.to_lowercase();
        self.themes
            .iter()
            .enumerate()
            .filter(|(_, t)| t.label().to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_theme(&self) -> Option<Theme> {
        let filtered = self.filtered_indices();
        let pos = self.selected.min(filtered.len().saturating_sub(1));
        filtered
            .get(pos)
            .and_then(|&idx| self.themes.get(idx).cloned())
    }

    pub fn select_next(&mut self) {
        let len = self.filtered_indices().len();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    pub fn select_previous(&mut self) {
        let len = self.filtered_indices().len();
        if len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or_else(|| len - 1);
        }
    }

    pub fn push_query(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
    }

    pub fn pop_query(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    #[test]
    fn test_theme_picker_navigation() {
        let mut picker = ThemePickerState::new(&Theme::Auto);
        assert!(!picker.themes.is_empty());
        assert_eq!(picker.selected, 0);

        // Filter themes list
        picker.query = "dark".to_owned();
        let indices = picker.filtered_indices();
        assert!(!indices.is_empty());

        let theme = picker.selected_theme();
        assert!(theme.is_some());

        // Select next
        picker.select_next();

        // Reset query
        picker.clear_query();
        assert_eq!(picker.query, "");
    }
}
