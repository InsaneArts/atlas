/// One of the top row's layouts, widest first (v3 V15): a narrowing window takes the first that fits
/// (``ViewerLayout/barLayouts``), so the status line goes first, then the tabs' words, then the actions' words. Nothing
/// truncates to a short word; the white capsule keeps its whole title.
///
/// The tabs stay centred, so the status's column is as wide as the buttons'. Only the widest layout asks for room for
/// the status line; the others show it only in whatever room the buttons' width leaves the status, cut short, and not
/// at all once that room is too small for a few words.
enum ViewerTopRowFit: Hashable, CaseIterable, Sendable {
  /// Everything: the status with room for its line, the tabs' words and the actions' words.
  case full
  /// The status line asks for no room; the tabs and actions keep their words.
  case statusWord
  /// The status line asks for no room and the tabs are symbols; the actions keep their words.
  case tabSymbols
  /// The status line asks for no room, and the tabs and the actions beside the white capsule are symbols.
  case actionSymbols

  /// Whether the layout asks for room for the status line beside its word.
  var reservesLine: Bool { self == .full }

  /// What a tab shows.
  var tabFace: TabsFace {
    switch self {
    case .full, .statusWord: .word
    case .tabSymbols, .actionSymbols: .symbol
    }
  }

  /// Whether Clear, Keys, Stop and Resume keep their words; the white capsule always does.
  var actionWords: Bool { self != .actionSymbols }
}
