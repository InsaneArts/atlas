/// The Activity pane's filters, in the order its tabs draw them.
package enum ViewerActivityFilter: Hashable, Sendable, CaseIterable {
  /// Every action.
  case all
  /// What agents did.
  case agent
  /// What the person did.
  case you
  /// What did not work.
  case problems
}
