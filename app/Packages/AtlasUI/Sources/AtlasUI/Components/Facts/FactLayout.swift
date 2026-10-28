import SwiftUI

/// A fact row's measures, which every row listed under a fact group's name shares, a fact's or a cleanup step's.
enum FactLayout {
  /// 6: above and below a row's text.
  static let inset = Spacing.xsPlus
  /// 1: how far a check's mark drops to centre on its name's first line.
  static let markDrop = Spacing.hairline
}
