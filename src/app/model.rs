#[derive(Debug, Default)]
pub struct ModelPickerState {
    pub models: Vec<String>,
    pub selected: usize,
}

impl ModelPickerState {
    pub fn new(models: Vec<String>, current_model: &str) -> Self {
        let selected = models
            .iter()
            .position(|model| model.trim_start_matches("models/") == current_model)
            .unwrap_or(0);

        Self { models, selected }
    }

    pub fn selected_model(&self) -> Option<String> {
        self.models.get(self.selected).cloned()
    }

    pub fn select_next(&mut self) {
        if !self.models.is_empty() {
            self.selected = (self.selected + 1) % self.models.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.models.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or_else(|| self.models.len() - 1);
        }
    }
}
