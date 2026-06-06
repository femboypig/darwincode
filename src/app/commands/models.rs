use crate::app::core::{App, SubmitAction};

pub fn run(app: &mut App) -> Option<SubmitAction> {
    app.begin_load_chat_models().map(SubmitAction::LoadModels)
}
