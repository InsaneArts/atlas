/// One tab in a capsule track: the value it stands for and its word.
package struct CapsuleTab<Tab: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// The value the owner keys on, which identifies the tab.
  package var id: Tab
  /// The tab's word, such as "Activity".
  package var title: String

  /// Creates a tab.
  ///
  /// - Parameters:
  ///   - id: The value the owner keys on.
  ///   - title: The tab's word.
  package init(id: Tab, title: String) {
    self.id = id
    self.title = title
  }
}
