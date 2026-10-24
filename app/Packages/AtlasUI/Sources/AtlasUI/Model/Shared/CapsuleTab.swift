/// One tab in a capsule track: the value it stands for and its word.
public struct CapsuleTab<Tab: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// The value the owner keys on, which identifies the tab.
  public var id: Tab
  /// The tab's word, such as "Activity".
  public var title: String

  /// Creates a tab.
  ///
  /// - Parameters:
  ///   - id: The value the owner keys on.
  ///   - title: The tab's word.
  public init(id: Tab, title: String) {
    self.id = id
    self.title = title
  }
}
