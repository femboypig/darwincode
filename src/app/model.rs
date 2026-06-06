#[derive(Debug, Default)]
pub struct ModelPickerState {
    pub models: Vec<String>,
    pub selected: usize,
    pub query: String,
}

impl ModelPickerState {
    pub fn new(models: Vec<String>, current_model: &str) -> Self {
        let selected = models
            .iter()
            .position(|model| model.trim_start_matches("models/") == current_model)
            .unwrap_or(0);

        Self {
            models,
            selected,
            query: String::new(),
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.query.is_empty() {
            return (0..self.models.len()).collect();
        }
        let q = self.query.to_lowercase();
        self.models
            .iter()
            .enumerate()
            .filter(|(_, m)| m.trim_start_matches("models/").to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_model(&self) -> Option<String> {
        let filtered = self.filtered_indices();
        let pos = self.selected.min(filtered.len().saturating_sub(1));
        filtered
            .get(pos)
            .and_then(|&idx| self.models.get(idx).cloned())
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

    #[test]
    fn test_model_picker_state_navigation() {
        let models = vec![
            "models/gemini-2.0-flash".to_owned(),
            "models/claude-3-5-sonnet".to_owned(),
            "models/gpt-4o".to_owned(),
        ];
        let mut picker = ModelPickerState::new(models, "claude-3-5-sonnet");
        assert_eq!(picker.selected, 1);
        assert_eq!(picker.selected_model().unwrap(), "models/claude-3-5-sonnet");

        picker.select_next();
        assert_eq!(picker.selected, 2);
        assert_eq!(picker.selected_model().unwrap(), "models/gpt-4o");

        picker.select_previous();
        assert_eq!(picker.selected, 1);

        picker.push_query('g');
        let filtered = picker.filtered_indices();
        assert_eq!(filtered.len(), 2); // gemini and gpt-4o

        picker.pop_query();
        assert_eq!(picker.query, "");

        picker.push_query('c');
        assert_eq!(picker.filtered_indices().len(), 1); // claude
        picker.clear_query();
        assert_eq!(picker.query, "");
    }
}
