//! Terminal questions and progress for the owned teardown operation.
use atlas::{
    error::Result,
    ui::{
        progress::{done, say, skipped},
        prompt,
    },
    uninstall::{self, Progress, Question},
};
use serde_json::Value;
use std::path::Path;

pub(super) async fn run(
    config: &Path,
    keep_config: bool,
    yes: bool,
    remove_helper: bool,
) -> Result<Value> {
    uninstall::run(
        config,
        keep_config,
        yes,
        remove_helper,
        &|question| {
            prompt::confirm(
                match question {
                    Question::Remove => "Remove Atlas?",
                    Question::ForceLogout => {
                        "An app in the helper is holding up the log-out. Log out anyway?"
                    }
                },
                false,
                "pass --yes",
            )
        },
        &|notice| match notice {
            Progress::Say(message) => say(message),
            Progress::Done(label, detail) => done(label, detail),
            Progress::Skipped(label, detail) => skipped(label, detail),
        },
    )
    .await
}
