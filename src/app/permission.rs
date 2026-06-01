use crate::config::PermissionLevel;

#[derive(Debug, Default)]
pub struct PermissionPickerState {
    pub selected: usize,
}

impl PermissionPickerState {
    pub fn options() -> Vec<(&'static str, &'static str, PermissionLevel)> {
        vec![
            ("Safe", "Read-only access to codebase", PermissionLevel::Safe),
            ("Guardian", "Always ask for confirmation", PermissionLevel::Guardian),
            ("Chaos", "Auto-execute everything", PermissionLevel::Chaos),
        ]
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1) % Self::options().len();
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.checked_sub(1).unwrap_or(Self::options().len() - 1);
    }
}
