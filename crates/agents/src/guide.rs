//! The guide agents read: its core, the references kept beside it, the
//! whole text `atlas agent-guide` prints, and the agent plugin's skill.
use std::sync::LazyLock;

/// The agent plugin's skill, as the plugin ships it: its core under a short
/// heading, then where its references are.
const SKILL: &str = include_str!("../../../agents/atlas/skills/atlas/SKILL.md");

/// The guide's core, what every agent reads first: the skill's body under the
/// guide's own heading.
pub(crate) static CORE: LazyLock<String> = LazyLock::new(|| {
    let (_, body) = SKILL.split_once("\n# Atlas\n\n").unwrap();
    let (body, _) = body.split_once("\n\n## References\n").unwrap();
    format!("# Atlas: guide for agents\n\n{body}\n")
});

/// What an agent reads only when a task needs it, in the skill's folder.
pub(crate) const REFERENCES: [&str; 5] = [
    "testing.md",
    "environment.md",
    "limits.md",
    "commands.md",
    "troubleshooting.md",
];

/// The whole guide `atlas agent-guide` prints: the core, then each
/// reference written for the guide, then where the rest is.
pub static GUIDE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}\n{}",
        *CORE,
        concat!(
            include_str!("../../../agents/atlas/skills/atlas/references/testing.md"),
            "\n",
            include_str!("../../../agents/atlas/skills/atlas/references/environment.md"),
            "\n",
            include_str!("../../../agents/atlas/skills/atlas/references/limits.md"),
            "\n",
            "## References\n\n`atlas agent-guide --commands` prints every command and its options, \
and `atlas agent-guide --errors` the error reference: every code and what to do.\n",
        )
    )
});

/// The error reference, printed by `atlas agent-guide --errors`.
pub const ERRORS: &str =
    include_str!("../../../agents/atlas/skills/atlas/references/troubleshooting.md");

/// The agent plugin's skill, as the plugin ships it.
pub fn skill() -> String {
    SKILL.to_owned()
}

#[cfg(test)]
#[path = "guide_tests.rs"]
mod tests;
