/// A run of a menu's items, set apart from the next by a separator, under its heading when it has one.
public struct ViewerMenuSection: Equatable, Sendable, Identifiable {
  /// The heading over its items, such as "Details"; nil for none.
  public var title: String?
  /// The section's items, in order.
  public var items: [ViewerMenuItem]

  /// The section's identity in its menu: its heading, or its first item's title.
  public var id: String { title ?? items.first?.title ?? "" }

  /// Creates a section.
  ///
  /// - Parameters:
  ///   - items: Its items, in order.
  ///   - title: The heading over them; none by default.
  public init(_ items: [ViewerMenuItem], title: String? = nil) {
    self.title = title
    self.items = items
  }
}
