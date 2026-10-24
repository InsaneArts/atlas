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
  /// 440: every setup screen's one column, so moving between screens never shifts its edges (v3 D2).
  static let column: CGFloat = 440
  /// 2: the most lines a line naming an account takes; a longer name is cut in its middle, so both its ends show.
  static let nameLines = 2
  /// 3: the most account rows the choice shows whole; with more, they scroll.
  static let accountRowsInView = 3
  /// 170: the height scrolling account rows take: two rows and most of a third, so the cut row shows there are more,
  /// with room left for the work note and the footer.
  static let accountListHeight: CGFloat = 170
  /// 28: above a screen's content, clear of the title bar (v3 `.stage2`).
  static let contentTop: CGFloat = 28
  /// 64: the footer's height under a screen (v3 D2).
  static let footerHeight: CGFloat = 64
  /// 56: the footer's height under the permission panel beside System Settings (v3 D2).
  static let sideFooterHeight: CGFloat = 56
  /// 20: the footer's side inset (v3 `.foot2`).
  static let barInset: CGFloat = 20
  /// 20: the work note's least height, so the screen does not jump as it comes and goes.
  static let workNoteHeight: CGFloat = 20
  /// 44: above the permission panel's content, clear of the title bar.
  static let panelTop: CGFloat = 44
  /// 18: below the permission panel's content.
  static let panelBottom: CGFloat = 18
  /// 104: the app icon on the welcome screen.
  static let welcomeIcon: CGFloat = 104
  /// 92: the app icon on the done screen.
  static let doneIcon: CGFloat = 92
  /// 72: the app icon on the helper's allow screen.
  static let allowIcon: CGFloat = 72
  /// 72: Atlas's icon in the permission panel, dragged into System Settings.
  static let dragIcon: CGFloat = 72
  /// 30: the hand carrying the drag ghost.
  static let ghostHand: CGFloat = 30
  /// The hand's symbol font, at ``ghostHand``: a symbol of its own size, not text on the type scale.
  static let ghostHandFont = Font.system(size: ghostHand)
  /// 14: how far past the ghost's icon, right and down, the hand holds it.
  static let ghostHandOffset: CGFloat = 14
  /// 40: the warning symbol on the blocked screen.
  static let blockedSymbol: CGFloat = 40
  /// The warning symbol's font, at ``blockedSymbol``: a symbol of its own size, not text on the type scale.
  static let blockedSymbolFont = Font.system(size: blockedSymbol)

  /// The one vertical rhythm every setup screen keeps, so a screen's pieces sit at the same heights from screen to
  /// screen (v3 D2). Each value is the space a piece takes above it, or, for the hero and the label, below it.
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
