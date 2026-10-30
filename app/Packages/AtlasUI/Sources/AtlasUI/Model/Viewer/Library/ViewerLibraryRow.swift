/// One of the Library's rows at the sidebar's foot: Activity, Screenshots or Apps with how many it holds, chosen while
/// its list shows.
public struct ViewerLibraryRow: Equatable, Sendable, Identifiable {
  /// The list the row shows.
  public var place: ViewerPlace
  /// The row's word, such as "Activity".
  public var title: String
  /// Its tooltip: its word with its View menu shortcut.
  public var help: String
  /// How many the list holds, such as today's actions, at the row's trailing edge; nil until it has been read.
  public var count: Int?
  /// What the list holds that the person has not seen, as a dot after the row's word.
  public var unseen: ViewerUnseen
  /// Whether the row's list shows now.
  public var isChosen: Bool

  /// The row's identity: its list.
  public var id: ViewerPlace { place }

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - place: The list the row shows.
  ///   - title: The row's word.
  ///   - help: Its tooltip, with its shortcut.
  ///   - count: How many the list holds; nil, unknown, by default.
  ///   - unseen: What the list holds unseen; nothing by default.
  ///   - isChosen: Whether the row's list shows now; no by default.
  public init(
    place: ViewerPlace,
    title: String,
    help: String,
    count: Int? = nil,
    unseen: ViewerUnseen = .none,
    isChosen: Bool = false
  ) {
    self.place = place
    self.title = title
    self.help = help
    self.count = count
    self.unseen = unseen
    self.isChosen = isChosen
  }
}
