/// One group of the Apps pane's cards, such as the apps agents opened or those already open: its heading, its count
/// and its cards, or one group with no heading while a single session's apps show.
public struct ViewerAppGroup: Equatable, Sendable, Identifiable {
  /// The group's identity, which stays while its cards change.
  public var id: String
  /// Its heading, such as "Opened by agents"; nil for none.
  public var title: String?
  /// Its apps.
  public var cards: [ViewerAppCard]

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - id: The group's identity.
  ///   - title: Its heading, or nil for none.
  ///   - cards: Its apps.
  public init(id: String, title: String?, cards: [ViewerAppCard]) {
    self.id = id
    self.title = title
    self.cards = cards
  }
}
