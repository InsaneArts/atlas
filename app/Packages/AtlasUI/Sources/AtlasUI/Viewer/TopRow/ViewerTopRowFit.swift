/// One of the top row's layouts, widest first: a narrowing window takes the first that fits
/// (``ViewerLayout/barLayouts``), so the status line goes first, then the tabs' words, then the picture's pause, then
/// the actions' words. Nothing truncates to a short word; the white capsule keeps its whole title. The pause goes
/// before Stop and Resume give up their words, so a narrow bar never shows a pause, a stop and a fast-forward side by
/// side, which would read as one player's buttons when the pause acts on the picture and the others on the agent; it
/// stays in the View menu and on ⇧⌘P.
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
  /// As ``tabSymbols``, without the picture's pause.
  case noPause
  /// The status line asks for no room, the tabs and the actions beside the white capsule are symbols, and the
  /// picture's pause is gone.
  case actionSymbols

  /// Whether the layout asks for room for the status line beside its word.
  var reservesLine: Bool { self == .full }

  /// What a tab shows.
  var tabFace: TabsFace {
    switch self {
    case .full, .statusWord: .word
    case .tabSymbols, .noPause, .actionSymbols: .symbol
    }
  }

  /// Whether Keys, Stop and Resume keep their words; the white capsule always does.
  var actionWords: Bool { self != .actionSymbols }

  /// Whether the picture's pause stands in the row.
  var showsPause: Bool { self != .noPause && self != .actionSymbols }
}
