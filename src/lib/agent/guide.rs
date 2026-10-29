//! The guide agents read: its core, the references kept beside it, the
//! whole text `atlas agent-guide` prints, and the agent plugin's skill.

/// The guide's core: what every agent reads first.
pub(crate) const CORE: &str = include_str!("../agent_guide.md");

/// What an agent reads only when a task needs it, in the skill's folder:
/// (file, what it holds).
pub(crate) const REFERENCES: &[(&str, &str)] = &[
    ("testing.md", "Test your own app."),
    (
        "environment.md",
        "Give the app what it needs: copy files in and roll them back.",
    ),
    (
        "limits.md",
        "Limits, and details of typing, keys, scrolling and `atlas run`.",
    ),
    ("commands.md", "every command and its options."),
    (
        "troubleshooting.md",
        "the error reference, every code and what to do.",
    ),
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

/// When a skill loads: what Atlas is for, in the words an agent matches.
pub(super) const SKILL_DESCRIPTION: &str = "See and use macOS GUIs in a hidden second login (the helper) \
without touching the person's desktop, windows, keyboard or mouse. Use it whenever you change \
something a person could see in a Mac app (a window, dialog, menu, menu bar item, layout or \
icon), open the build in the helper and check it before saying it works. Also use it to \
reproduce on-screen bugs, run installers or anything that opens windows or takes focus, test \
a web app in Google Chrome or Firefox, take screenshots, and click or type in apps. Also use it when the person \
asks to watch the helper, see what you are doing or take over: open the live view for them. Not for unit tests, builds \
or reading files.";

/// The agent plugin's skill: the core under the skill's own title, then
/// the references it points to. The plugin's SKILL.md is this text (a test
/// keeps them equal).
pub fn skill() -> String {
    let core = CORE
        .split_once('\n')
        .map_or(CORE, |(_, rest)| rest)
        .trim_start();
    let references: String = REFERENCES
        .iter()
        .map(|(file, about)| format!("- [references/{file}](references/{file}): {about}\n"))
        .collect();

    format!(
        "---\nname: atlas\ndescription: \"{SKILL_DESCRIPTION}\"\n---\n\n# Atlas\n\n\
{core}\n## References\n\nRead one when the task needs it:\n\n{references}"
    )
}
