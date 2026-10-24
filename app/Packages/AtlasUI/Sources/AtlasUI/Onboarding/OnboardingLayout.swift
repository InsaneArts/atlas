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
  /// 420: the done screen's column.
  static let narrowColumn: CGFloat = 420
  /// 440: a screen's column unless it says otherwise.
  static let column: CGFloat = 440
  /// 450: the helper's done screen's column.
  static let helperDoneColumn: CGFloat = 450
  /// 460: a column holding a list of rows: switching, agents, permissions.
  static let listColumn: CGFloat = 460
  /// 480: the widest column: the account choice and the helper's allow screen.
  static let wideColumn: CGFloat = 480
  /// 2: the most lines a line naming an account takes; a longer name is cut in its middle, so both its ends show.
  static let nameLines = 2
  /// 3: the most account rows the choice shows whole; with more, they scroll.
  static let accountRowsInView = 3
  /// 170: the height scrolling account rows take: two rows and most of a third, so the cut row shows there are more,
  /// with room left for the work note and the footer.
  static let accountListHeight: CGFloat = 170
  /// 32: how far a scrolling screen's content fades out above its pinned bottom, so a cut line reads as more below.
  static let scrollFade: CGFloat = 32
  /// 36: above a screen's content, clear of the title bar.
  static let contentTop: CGFloat = 36
  /// 76: the footer's height.
  static let footerHeight: CGFloat = 76
  /// 22: the footer's side inset.
  static let barInset: CGFloat = 22
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
  /// 14: how far below its place a screen starts as it arrives.
  static let entryOffset: CGFloat = 14
  /// −10: how far above its place a screen ends as it leaves.
  static let exitOffset: CGFloat = -10
  /// 0.08 seconds: a screen arrives just after the last one starts to leave.
  static let entryDelay: TimeInterval = 0.08
}
