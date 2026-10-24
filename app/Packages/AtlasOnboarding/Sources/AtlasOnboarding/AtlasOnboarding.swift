/// AtlasOnboarding — level 2, its own package (`app/Packages/AtlasOnboarding`), above AtlasCommands.
///
/// Owns: the rules that choose each setup step: the facts `atlas onboarding status` reports (`Facts/`), the steps and
/// their progress (`Steps/`), the account name and password rules and the person's choices (`Account/`), the agents
/// to connect (`Agents/`) and the permissions the helper needs (`Permissions/`). Every decision here is a pure
/// function with a test.
/// Never imports: anything but Foundation and AtlasCommands.
/// Seams: none. The app runs the commands and hands it their replies.
