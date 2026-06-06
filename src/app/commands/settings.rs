use crate::app::core::App;

pub fn run(app: &mut App) {
    app.open_setup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_settings_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.screen = crate::app::Screen::Chat;
        run(&mut app);
        assert_eq!(app.ui.screen, crate::app::Screen::Setup);
    }
}
