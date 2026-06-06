use super::chat::ChatCommand;
use super::core::{App, SubmitAction};

impl App {
    pub fn run_command(&mut self, command: ChatCommand) -> Option<SubmitAction> {
        dispatch(self, command)
    }
}

pub fn dispatch(app: &mut App, command: ChatCommand) -> Option<SubmitAction> {
    match command {
        ChatCommand::Plan => {
            super::commands::plan::run(app);
            None
        }
        ChatCommand::Build => {
            super::commands::build::run(app);
            None
        }
        ChatCommand::Settings => {
            super::commands::settings::run(app);
            None
        }
        ChatCommand::Exit => {
            super::commands::exit::run(app);
            None
        }
        ChatCommand::Models => super::commands::models::run(app),
        ChatCommand::Theme => {
            super::commands::theme::run(app);
            None
        }
        ChatCommand::Agents => {
            super::commands::agents::run_picker(app);
            None
        }
        ChatCommand::Agent(name) => {
            super::commands::agents::run_agent(app, name);
            None
        }
        ChatCommand::Custom(name) => {
            super::commands::custom::run(app, name);
            None
        }
        ChatCommand::Permissions(level) => super::commands::permissions::run(app, level),
        ChatCommand::Resume(session_id) => {
            super::commands::resume::run(app, session_id);
            None
        }
        ChatCommand::Clear => {
            super::commands::clear::run(app);
            None
        }
        ChatCommand::New => {
            super::commands::new::run(app);
            None
        }
        ChatCommand::History => {
            super::commands::history::run(app);
            None
        }
        ChatCommand::Undo => {
            super::commands::undo::run(app);
            None
        }
        ChatCommand::Shell(session_arg) => {
            super::commands::shell::run(app, session_arg);
            None
        }
        ChatCommand::Help => {
            super::commands::help::run(app);
            None
        }
        ChatCommand::Unknown(command) => {
            super::commands::unknown::run(app, command);
            None
        }
    }
}
