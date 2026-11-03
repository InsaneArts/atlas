//! Terminal questions and progress for the owned teardown operation.
use super::{cli::Uninstall, prompt};
use atlas_cli::{
    ui::progress::{done, say, skipped},
    uninstall::{self, Progress, Question},
};
use atlas_core::error::Result;
use serde_json::Value;
use std::path::Path;

pub(super) async fn run(config: &Path, args: Uninstall) -> Result<Value> {
    super::dispatch::as_yourself("uninstall")?;
    uninstall::run(
        config,
        args.keep_config,
        args.yes,
        args.remove_helper,
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
