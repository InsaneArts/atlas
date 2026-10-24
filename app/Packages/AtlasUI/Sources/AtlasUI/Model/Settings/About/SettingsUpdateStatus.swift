/// What About says about updates, in the one spot under its buttons.
public enum SettingsUpdateStatus: Hashable, Sendable {
  /// A check runs: a spinner and the words, such as "Checking for updates…".
  case working(String)
  /// Nothing newer, or the update is done: a green check and the words, such as "Atlas is up to date.".
  case done(String)
  /// A newer version is published: "Atlas 0.2.0 is available.".
  case available(String)
  /// The newer version installs: a progress bar over "Installing Atlas 0.2.0…".
  case installing(String)
  /// A check or an install failed, saying why: the problem line.
  case problem(String)
  /// The install failed because Atlas isn't where the helper can run it: the problem line, such as "Move Atlas to
  /// the Applications folder, then try again.", with Show in Finder.
  case misplaced(String)
  /// Plain words, such as "Atlas has no public release yet. Check atlas.insanearts.io for news.", whose links are
  /// written as Markdown and shown live.
  case note(String)

  /// The words, whatever the kind.
  public var text: String {
    switch self {
    case .working(let text), .done(let text), .available(let text), .installing(let text), .problem(let text),
      .misplaced(let text), .note(let text):
      text
    }
  }
}
