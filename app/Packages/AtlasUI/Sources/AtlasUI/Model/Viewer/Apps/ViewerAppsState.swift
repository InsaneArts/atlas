/// The Apps pane: how many apps and windows are open, then the apps as cards in groups, by who opened them while every
/// session's work shows, or only the chosen session's own; or why none shows. The sidebar's search narrows the apps.
public struct ViewerAppsState: Equatable, Sendable {
  /// How many apps are open, as the summary's title says it, such as "5 apps open"; empty while none shows.
  public var title: String
  /// How many windows they have, beside the title, such as "8 windows"; empty while no app shows.
  public var detail: String
  /// The apps in their groups, each group's cards in a stable order.
  public var groups: [ViewerAppGroup]
  /// Why no app shows, in place of them; nil while apps show or before the first reading.
  public var empty: EmptyMessageState?

  /// Creates the Apps pane's state.
  ///
  /// - Parameters:
  ///   - title: How many apps are open.
  ///   - detail: How many windows they have.
  ///   - groups: The apps in their groups.
  ///   - empty: Why no app shows.
  public init(title: String, detail: String, groups: [ViewerAppGroup], empty: EmptyMessageState? = nil) {
    self.title = title
    self.detail = detail
    self.groups = groups
    self.empty = empty
  }

  /// What VoiceOver says of the pane, such as "5 apps open · 8 windows"; empty while no app shows.
  public var meta: String {
    detail.isEmpty ? title : "\(title) · \(detail)"
  }

  /// Every card, group by group.
  public var cards: [ViewerAppCard] {
    groups.flatMap(\.cards)
  }

  /// The pane with no window's picture, each card's picture its screen map, as the pane is kept once it has hidden,
  /// so no picture of the helper's screen outlives Apps showing.
  public var forgettingPictures: ViewerAppsState {
    var forgotten = self
    for group in forgotten.groups.indices {
      for card in forgotten.groups[group].cards.indices {
        forgotten.groups[group].cards[card].picture?.window = nil
      }
    }
    return forgotten
  }
}
