/// One action taken in the helper, as a caption over the desktop or a row of the panel's Recent shows it.
package struct ActivityEntry: Equatable, Sendable, Identifiable {
  /// The action's id from the service, which holds as newer actions arrive.
  package var id: String
  /// Who did what, in one line, such as "You clicked in Opie".
  package var line: String
  /// How long ago, in words; empty when unknown.
  package var when: String
  /// The mark that leads it: a warning sign when it did not work, else a quiet dot.
  package var mark: ActivityMarkKind
  /// Whether it is the newest action in its list, which a caption marks with a breathing dot.
  package var isNewest: Bool

  /// Creates an action as a caption or a row shows it.
  ///
  /// - Parameters:
  ///   - id: The action's id from the service.
  ///   - line: Who did what, in one line.
  ///   - when: How long ago, in words; empty when unknown.
  ///   - mark: The mark that leads it.
  ///   - isNewest: Whether it is the newest action in its list.
  package init(id: String, line: String, when: String, mark: ActivityMarkKind = .quiet, isNewest: Bool) {
    self.id = id
    self.line = line
    self.when = when
    self.mark = mark
    self.isNewest = isNewest
  }
}
