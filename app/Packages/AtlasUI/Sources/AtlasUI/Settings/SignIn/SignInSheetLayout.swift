import CoreGraphics

/// Sign In to a Site…'s sheet's measures: an alert card's family, wider for its field.
enum SignInSheetLayout {
  /// 380: the card's width.
  static let width: CGFloat = 380
  /// 20: the card's inset round its content.
  static let inset = Spacing.xl
  /// 6: between the title and the note.
  static let titleGap = Spacing.xsPlus
  /// 16: between the note, the field and the buttons.
  static let gap = Spacing.l
  /// 8: between the buttons, and inside the field before its text.
  static let buttonGap = Spacing.s
  /// 10: the field's text inset from its ends.
  static let fieldInset = Spacing.sPlus
  /// 3: the field's halo while focused, as a search field's.
  static let haloWidth = SearchFieldLayout.haloWidth
}
