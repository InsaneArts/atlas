/// One label the footer's main capsule takes: its title and the symbol before it, such as Take Control's hand.
public struct ViewerControlLabel: Equatable, Sendable {
  /// The title.
  public var title: String
  /// The symbol before the title; nil draws the title alone.
  public var symbol: SymbolName?

  /// Creates a label.
  ///
  /// - Parameters:
  ///   - title: The title.
  ///   - symbol: The symbol before the title; none by default.
  public init(_ title: String, symbol: SymbolName? = nil) {
    self.title = title
    self.symbol = symbol
  }
}
