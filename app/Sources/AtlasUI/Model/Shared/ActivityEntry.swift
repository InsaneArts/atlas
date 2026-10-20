/// One action taken in the helper, as a sidebar row or a caption over the desktop shows it.
package struct ActivityEntry: Equatable, Sendable, Identifiable {
  /// The action's place in the service's list of recent actions, newest first.
  package var id: Int
  /// The actor's initial, for the row's disc.
  package var initial: String
  /// Who did what, in one line, such as "Agent ran a command".
  package var line: String
  /// How long ago, in words; empty when unknown.
  package var when: String
  /// Whose action it was, which its disc's colour shows: a failure first, then the person's own, then anyone else's.
  package var kind: ActivityKind
  /// Whether it is the newest action in its list, which a caption marks with a breathing dot.
  package var isNewest: Bool

  /// Creates an action as a row or a caption shows it.
  ///
  /// - Parameters:
  ///   - id: The action's place in the service's list of recent actions, newest first.
  ///   - initial: The actor's initial, for the row's disc.
  ///   - line: Who did what, in one line, such as "Agent ran a command".
  ///   - when: How long ago, in words; empty when unknown.
  ///   - kind: Whose action it was, which its disc's colour shows: a failure first, then the person's own, then
  ///     anyone else's.
  ///   - isNewest: Whether it is the newest action in its list, which a caption marks with a breathing dot.
  package init(id: Int, initial: String, line: String, when: String, kind: ActivityKind, isNewest: Bool) {
    self.id = id
    self.initial = initial
    self.line = line
    self.when = when
    self.kind = kind
    self.isNewest = isNewest
  }
}
