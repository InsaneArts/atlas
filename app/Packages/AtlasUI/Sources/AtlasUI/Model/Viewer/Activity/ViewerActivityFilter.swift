/// The Activity pane's filters, in the order its tabs draw them.
public enum ViewerActivityFilter: Hashable, Sendable, CaseIterable {
  /// Every action.
  case all
  /// What did not work.
  case problems
  /// The screenshots taken.
  case screenshots
  /// The text typed and the keys pressed.
  case typing
}
