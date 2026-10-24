import CoreGraphics

/// The sizes an SF Symbol takes, one per role (v3 D13): every symbol is drawn at its text's weight through
/// `Typography.symbol(_:weight:)`, never at a size of its own. v3 names the first `bar`; it is named here for what
/// it sits on, a control.
enum SymbolScale: CaseIterable, Sendable {
  /// 13: a control's symbol, such as a bar's circle button or a capsule's leading symbol.
  case control
  /// 12: a row's symbol, such as a tile's in a list or Activity.
  case row
  /// 19: a tab's symbol over its word, such as Settings' tabs.
  case tab

  /// The symbol's size in points.
  var pointSize: CGFloat {
    switch self {
    case .control: 13
    case .row: 12
    case .tab: 19
    }
  }
}
