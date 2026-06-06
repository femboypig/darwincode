use crate::app::core::App;

pub fn run(app: &mut App) {
    app.should_quit = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_exit_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.should_quit = false;
        run(&mut app);
        assert!(app.should_quit);
    }
}
