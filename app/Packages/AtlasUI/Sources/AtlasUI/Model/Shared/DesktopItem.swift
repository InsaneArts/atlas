/// One desktop in the menu that picks the desktop Atlas shows, as the viewer's sidebar foot and the panel's header open
/// it: its title, its dot in its state's tone and mark and its state's word under it, checked while it is the one shown.
public struct DesktopItem: Equatable, Sendable, Identifiable {
  /// The desktop's ID, which picking it names.
  public var id: String
  /// Its title, such as "Second Login" or "work".
  public var title: String
  /// Its state in a word, such as "Working".
  public var word: String
  /// Its dot's tone.
  public var tone: StatusTone
  /// Its dot's mark.
  public var mark: StatusMark
  /// Whether Atlas shows it now, which checks it.
  public var isViewed: Bool

  /// Creates an item.
  ///
  /// - Parameters:
  ///   - id: The desktop's ID.
  ///   - title: Its title.
  ///   - word: Its state in a word.
  ///   - tone: Its dot's tone.
  ///   - mark: Its dot's mark; a dot by default.
  ///   - isViewed: Whether Atlas shows it now; no by default.
  public init(
    id: String,
    title: String,
    word: String,
    tone: StatusTone,
    mark: StatusMark = .dot,
    isViewed: Bool = false
  ) {
    self.id = id
    self.title = title
    self.word = word
    self.tone = tone
    self.mark = mark
    self.isViewed = isViewed
  }
}
