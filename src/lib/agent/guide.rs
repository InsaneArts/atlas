//! The guide agents read: its core, the references kept beside it, the
//! whole text `atlas agent-guide` prints, and the agent plugin's skill.

/// The guide's core: what every agent reads first.
#[cfg(test)]
pub(crate) const CORE: &str = include_str!("../agent_guide.md");

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
pub const GUIDE: &str = concat!(
    include_str!("../agent_guide.md"),
    "\n",
    include_str!("../../../agents/atlas/skills/atlas/references/testing.md"),
    "\n",
    include_str!("../../../agents/atlas/skills/atlas/references/environment.md"),
    "\n",
    include_str!("../../../agents/atlas/skills/atlas/references/limits.md"),
    "\n",
    "## References\n\n`atlas agent-guide --commands` prints every command and its options, \
and `atlas agent-guide --errors` the error reference: every code and what to do.\n",
);

/// The error reference, printed by `atlas agent-guide --errors`.
pub const ERRORS: &str =
    include_str!("../../../agents/atlas/skills/atlas/references/troubleshooting.md");

/// The agent plugin's skill, as the plugin ships it.
pub fn skill() -> String {
    include_str!("../../../agents/atlas/skills/atlas/SKILL.md").to_owned()
}
