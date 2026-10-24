import CoreGraphics
import Foundation
import SwiftUI

/// The onboarding window's own measures, which no other window shares.
public enum OnboardingLayout {
  /// 720 by 600: the window as a card in the middle of the screen, tall enough for the account form and why macOS
  /// asks to approve it.
  public static let card = CGSize(width: 720, height: 600)
  /// 360 by 500: the window as a narrow panel beside System Settings.
  public static let side = CGSize(width: 360, height: 500)
  /// 440: every setup screen's one column, so moving between screens never shifts its edges.
  static let column: CGFloat = 440
  /// 2: the most lines a line naming an account takes; a longer name is cut in its middle, so both its ends show.
  static let nameLines = 2
  /// 3: the lines the account choice keeps for its footnote, whichever footnote shows, so picking an account never
  /// moves the screen.
  static let footnoteLines = 3
  /// 28: above a screen's content, clear of the title bar.
  static let contentTop: CGFloat = 28
  /// 72: below the title bar, where every screen's first piece starts, 100 points from the window's top, the same on
  /// every screen, so the screens' tops line up and nothing on a screen moves when a line appears below. The account
  /// form, its two-line working line and why macOS asks for a password still end with room to spare above the footer.
  static let columnTop: CGFloat = 72
  /// 24: the least room a screen leaves between its last piece and the footer, every reserved line counted.
  static let spare: CGFloat = 24
  /// 64: the footer's height under a screen.
  static let footerHeight: CGFloat = 64
  /// 56: the footer's height under the permission panel beside System Settings.
  static let sideFooterHeight: CGFloat = 56
  /// 20: the footer's side inset.
  static let barInset: CGFloat = 20
  /// 88: the app icon's visible tile on the welcome screen.
  static let welcomeTile: CGFloat = 88
  /// 80: the app icon's visible tile on the done screen.
  static let doneTile: CGFloat = 80
  /// 312: the permission panel's one column beside System Settings.
  static let sideColumn: CGFloat = 312
  /// 48: Atlas's visible tile in the helper's allow pairing, the asking account's picture's size.
  static let pairingTile: CGFloat = 48
  /// 64: Atlas's visible tile in the permission panel, dragged into System Settings, and the drag ghost's.
  static let dragTile: CGFloat = 64
  /// 84: the place at a permission row's end for where it stands, the same for every word, so a row's title never
  /// moves as the word changes.
  static let permissionStanding: CGFloat = 84
  /// 26: the hand carrying the drag ghost, twice a control's symbol.
  static let ghostHand = SymbolScale.control.pointSize * 2
  /// The hand's symbol font, at ``ghostHand``: a symbol of its own size, not text on the type scale.
  static let ghostHandFont = Font.system(size: ghostHand)
  /// 14: how far past the ghost's icon, right and down, the hand holds it.
  static let ghostHandOffset: CGFloat = 14

  /// The one vertical rhythm every setup screen keeps, so a screen's pieces sit at the same heights from screen to
  /// screen. Each value is the space a piece takes above it, or, for the hero and the label, below it.
  enum Rhythm {
    /// 18: below the hero: an icon, a pairing or a mark.
    static let hero: CGFloat = 18
    /// 6: below the label over the title.
    static let label: CGFloat = 6
    /// 10: between the title and the paragraph under it.
    static let title: CGFloat = 10
    /// 22: above the screen's main block, below the paragraph or the title.
    static let paragraph: CGFloat = 22
    /// 14: above the status line, below the main block.
    static let content: CGFloat = 14
    /// 10: above a note, below the status line.
    static let status: CGFloat = 10
  }
}
