/// The helper's health at the sidebar's foot, drawn as one of its rows: Atlas's mark with a dot in its tone, a title
/// over a line, such as "Atlas is ready" over "Helper atlas · 1440 × 900", and the menu a click opens, every fact
/// about the helper and then its commands.
public struct ViewerHelperLine: Equatable, Sendable {
  /// The dot's tone: ready while the helper can act, attention while it needs the person or setup, quiet while the
  /// window connects, which also dims the mark.
  public var tone: StatusTone
  /// The title, such as "Atlas is ready".
  public var title: String
  /// The line under it, such as "Helper atlas · 1440 × 900".
  public var line: String
  /// The line's tone: attention while it says what the person must do, quiet otherwise.
  public var lineTone: StatusTone
  /// The menu a click opens: the helper's facts, then Check Again, Open Settings on Helper…, Recover…, Switch… and
  /// Log Out…; none opens no menu.
  public var menu: [ViewerMenuSection]

  /// Creates the helper's row.
  ///
  /// - Parameters:
  ///   - tone: The dot's tone.
  ///   - title: The title.
  ///   - line: The line under it.
  ///   - lineTone: The line's tone; quiet by default.
  ///   - menu: The menu a click opens; none by default.
  public init(
    tone: StatusTone,
    title: String,
    line: String,
    lineTone: StatusTone = .quiet,
    menu: [ViewerMenuSection] = []
  ) {
    self.tone = tone
    self.title = title
    self.line = line
    self.lineTone = lineTone
    self.menu = menu
  }
}
