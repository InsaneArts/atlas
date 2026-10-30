/// One action taken in the helper, as a caption over the desktop or a row of the panel's Recent shows it.
public struct ActivityEntry: Equatable, Sendable, Identifiable {
  /// The action's id from the service, which holds as newer actions arrive.
  public var id: String
  /// Who did what, in one line, such as "You clicked in Opie".
  public var line: String
  /// How long ago, in words; empty when unknown.
  public var when: String
  /// The mark that leads it: a warning sign when it did not work, else a quiet dot.
  public var mark: ActivityMarkKind
  /// Whether it is the newest action in its list, which a caption marks with a breathing dot.
  public var isNewest: Bool
  /// The icon of the host session that did it, which a caption draws before its words while sessions show; nil for
  /// none.
  public var icon: RowIcon?

  /// Creates an action as a caption or a row shows it.
  ///
  /// - Parameters:
  ///   - id: The action's id from the service.
  ///   - line: Who did what, in one line.
  ///   - when: How long ago, in words; empty when unknown.
  ///   - mark: The mark that leads it.
  ///   - isNewest: Whether it is the newest action in its list.
  ///   - icon: The icon of the host session that did it; none by default.
  public init(
    id: String,
    line: String,
    when: String,
    mark: ActivityMarkKind = .quiet,
    isNewest: Bool,
    icon: RowIcon? = nil
  ) {
    self.id = id
    self.line = line
    self.when = when
    self.mark = mark
    self.isNewest = isNewest
    self.icon = icon
  }
}
