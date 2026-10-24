/// One tab in a row of tabs: the value it stands for, its word, and the symbol drawn for it in a narrow window or over
/// its word.
public struct CapsuleTab<Tab: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// The value the owner keys on, which identifies the tab.
  public var id: Tab
  /// The tab's word, such as "Activity".
  public var title: String
  /// The tab's symbol, such as `list.bullet` for Activity; nil draws its word alone.
  public var symbol: SymbolName?

  /// Creates a tab.
  ///
  /// - Parameters:
  ///   - id: The value the owner keys on.
  ///   - title: The tab's word.
  ///   - symbol: The tab's symbol; none by default.
  public init(id: Tab, title: String, symbol: SymbolName? = nil) {
    self.id = id
    self.title = title
    self.symbol = symbol
  }
}
