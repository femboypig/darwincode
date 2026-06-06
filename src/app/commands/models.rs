use crate::app::core::{App, SubmitAction};

pub fn run(app: &mut App) -> Option<SubmitAction> {
    app.begin_load_chat_models().map(SubmitAction::LoadModels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_models_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        let action = run(&mut app);
        assert!(action.is_some());
    }
}
