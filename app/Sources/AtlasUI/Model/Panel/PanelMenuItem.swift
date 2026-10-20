/// How one of the panel's optional menu rows shows: not at all, or shown and enabled or not.
package enum PanelMenuItem: Equatable, Sendable {
  /// The row does not show.
  case hidden
  /// The row shows and can be chosen.
  case enabled
  /// The row shows but waits.
  case disabled
}
