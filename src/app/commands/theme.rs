use crate::app::core::App;
use crate::app::theme_picker::ThemePickerState;

pub fn run(app: &mut App) {
    app.ui.theme_picker = ThemePickerState::new(&app.chat.config.theme);
    app.ui.theme_picker_open = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_theme_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.theme_picker_open = false;
        run(&mut app);
        assert!(app.ui.theme_picker_open);
    }
}
