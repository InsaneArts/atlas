/// One tab in a row of tabs: the value it stands for, its word, the symbol drawn for it in a narrow window or over its
/// word, how many things it holds, and its tooltip.
public struct CapsuleTab<Tab: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// The value the owner keys on, which identifies the tab.
  public var id: Tab
  /// The tab's word, such as "Activity".
  public var title: String
  /// The tab's symbol, such as `list.bullet` for Activity; nil draws its word alone.
  public var symbol: SymbolName?
  /// The tab's tooltip, such as "Activity (⌘2)", which names its shortcut; nil gives a tab drawn as its symbol its
  /// word, and a tab drawn as its word none.
  public var help: String?
  /// How many things it holds, drawn quietly after its word, such as Problems' 1; nil draws none.
  public var count: Int?

  /// Creates a tab.
  ///
  /// - Parameters:
  ///   - id: The value the owner keys on.
  ///   - title: The tab's word.
  ///   - symbol: The tab's symbol; none by default.
  ///   - help: The tab's tooltip; none by default.
  ///   - count: How many things it holds; none by default.
  public init(id: Tab, title: String, symbol: SymbolName? = nil, help: String? = nil, count: Int? = nil) {
    self.id = id
    self.title = title
    self.symbol = symbol
    self.help = help
    self.count = count
  }
}
