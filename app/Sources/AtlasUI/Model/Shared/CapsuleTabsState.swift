/// A row of tabs in a capsule track: the tabs, which one is selected, and the row's name for VoiceOver.
package struct CapsuleTabsState<Tab: Hashable & Sendable>: Equatable, Sendable {
  /// The tabs, in drawing order.
  package var tabs: [CapsuleTab<Tab>]
  /// The selected tab; one no tab has selects none.
  package var selected: Tab
  /// The row's name, which VoiceOver reads, such as "What you see".
  package var label: String

  /// Creates the row's state.
  ///
  /// - Parameters:
  ///   - tabs: The tabs, in drawing order.
  ///   - selected: The selected tab.
  ///   - label: The row's name, which VoiceOver reads.
  package init(tabs: [CapsuleTab<Tab>], selected: Tab, label: String) {
    self.tabs = tabs
    self.selected = selected
    self.label = label
  }
}
