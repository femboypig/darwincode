use crate::app::core::App;
use crate::app::theme_picker::ThemePickerState;

pub fn run(app: &mut App) {
    app.ui.theme_picker = ThemePickerState::new(&app.chat.config.theme);
    app.ui.theme_picker_open = true;
}
