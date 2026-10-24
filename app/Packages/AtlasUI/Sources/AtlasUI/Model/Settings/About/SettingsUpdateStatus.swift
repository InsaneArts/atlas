/// What About says about updates, in the one spot under its buttons (v3 G9).
public enum SettingsUpdateStatus: Hashable, Sendable {
  /// Nothing newer, or the update is done: a green check and the words, such as "Atlas is up to date.".
  case done(String)
  /// A newer version is published: "Atlas 0.2.0 is available.".
  case available(String)
  /// The newer version installs: a progress bar over "Installing Atlas 0.2.0…".
  case installing(String)
  /// A check or an install failed, saying why: the problem line.
  case problem(String)
  /// Plain small print, such as "Atlas has no public release yet. Check atlas.insanearts.io for news.".
  case note(String)

  /// The words, whatever the kind.
  public var text: String {
    switch self {
    case .done(let text), .available(let text), .installing(let text), .problem(let text), .note(let text): text
    }
  }
}
