use crate::config::PermissionLevel;

#[derive(Debug, Default)]
pub struct PermissionPickerState {
    pub selected: usize,
}

impl PermissionPickerState {
    pub fn options() -> Vec<(&'static str, &'static str, PermissionLevel)> {
        vec![
            (
                "Safe",
                "Read-only access to codebase",
                PermissionLevel::Safe,
            ),
            (
                "Guardian",
                "Always ask for confirmation",
                PermissionLevel::Guardian,
            ),
            ("Chaos", "Auto-execute everything", PermissionLevel::Chaos),
        ]
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1) % Self::options().len();
    }

    pub fn select_previous(&mut self) {
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or(Self::options().len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_picker_state() {
        let mut picker = PermissionPickerState::default();
        assert_eq!(picker.selected, 0);

        picker.select_next();
        assert_eq!(picker.selected, 1);

        picker.select_next();
        assert_eq!(picker.selected, 2);

        picker.select_next();
        assert_eq!(picker.selected, 0); // wrap around

        picker.select_previous();
        assert_eq!(picker.selected, 2); // wrap around backwards
    }
}
