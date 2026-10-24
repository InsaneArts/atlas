/// One tab in a row of icon tabs: the value it stands for, its word and its icon.
public struct IconTab<Tab: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// The value the owner keys on, which identifies the tab.
  public var id: Tab
  /// The tab's word, such as "General".
  public var title: String
  /// Its icon, over the word.
  public var icon: TabIcon

  /// Creates a tab.
  ///
  /// - Parameters:
  ///   - id: The value the owner keys on.
  ///   - title: The tab's word.
  ///   - icon: Its icon, over the word.
  public init(id: Tab, title: String, icon: TabIcon) {
    self.id = id
    self.title = title
    self.icon = icon
  }
}
