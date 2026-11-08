import AtlasCommands

/// What the Settings window knows of the person's files and the helper: the home folder as last read, whether agents
/// may copy sign-ins in, whether a change runs, and what the last one said.
struct SettingsFiles: Equatable {
  /// The home folder as `atlas onboarding files` last said; nil until it has.
  var home: HomeFiles?
  /// Whether agents may copy sign-ins in, as last said by either command; nil until one has.
  var signInCopies: Bool?
  /// Whether a change runs now.
  var changing = false
  /// Whether the home folder's own change runs now, which its button shows.
  var changingHome = false
  /// What the last change or reading said when it failed; nil for nothing.
  var problem: String?
  /// Counts the readings started and the changes started and ended, so only the newest reading's answer is kept: one
  /// a later reading or a change overtook may be older.
  var generation = 0
}
