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
