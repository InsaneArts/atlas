/// The helper's health at the sidebar's foot: a dot in its tone and a few words, such as "Helper ready · atlas", with
/// every fact about the helper in its tooltip and the helper's commands in the menu a click opens.
public struct ViewerHelperLine: Equatable, Sendable {
  /// The dot's tone: ready while the helper can act, attention while it needs the person or setup, quiet while it is
  /// not known yet.
  public var tone: StatusTone
  /// The words, such as "Helper ready · atlas" or "Helper needs setup · 2 checks".
  public var words: String
  /// Its tooltip, one fact a line, such as "Display: 1440 × 900".
  public var help: String
  /// The menu a click opens: Check Again, Open Settings on Helper…, Recover…, Switch… and Log Out…; none opens no
  /// menu.
  public var menu: [ViewerMenuSection]

  /// Creates the helper's line.
  ///
  /// - Parameters:
  ///   - tone: The dot's tone.
  ///   - words: The words.
  ///   - help: Its tooltip, one fact a line; none by default.
  ///   - menu: The menu a click opens; none by default.
  public init(tone: StatusTone, words: String, help: String = "", menu: [ViewerMenuSection] = []) {
    self.tone = tone
    self.words = words
    self.help = help
    self.menu = menu
  }
}
