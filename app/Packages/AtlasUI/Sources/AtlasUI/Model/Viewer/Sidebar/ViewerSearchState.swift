/// The sidebar's search: a circle in its top bar until opened, then a field that keeps the sessions whose name or line
/// holds the query.
public struct ViewerSearchState: Equatable, Sendable {
  /// Whether the field is open.
  public var isOpen: Bool
  /// What the person typed; empty keeps every session.
  public var query: String
  /// Counts the asks for the field's focus, as Find makes them: each change takes it.
  public var focusRequests: Int

  /// Creates the search's state.
  ///
  /// - Parameters:
  ///   - isOpen: Whether the field is open; closed by default.
  ///   - query: What the person typed; nothing by default.
  ///   - focusRequests: The asks for the field's focus so far; none by default.
  public init(isOpen: Bool = false, query: String = "", focusRequests: Int = 0) {
    self.isOpen = isOpen
    self.query = query
    self.focusRequests = focusRequests
  }
}
