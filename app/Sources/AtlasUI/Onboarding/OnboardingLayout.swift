import CoreGraphics
import Foundation

/// The onboarding window's own measures, which no other window shares.
package enum OnboardingLayout {
  /// 720 by 540: the window as a card in the middle of the screen.
  package static let card = CGSize(width: 720, height: 540)
  /// 360 by 500: the window as a narrow panel beside System Settings.
  package static let side = CGSize(width: 360, height: 500)
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
  /// 36: above a screen's content, clear of the title bar.
  static let contentTop: CGFloat = 36
  /// 76: the footer's height.
  static let footerHeight: CGFloat = 76
  /// 22: the footer's side inset.
  static let barInset: CGFloat = 22
  /// 20: the work note's least height, so the screen does not jump as it comes and goes.
  static let workNoteHeight: CGFloat = 20
  /// 26: the guide panel's side inset.
  static let guideSideInset: CGFloat = 26
  /// 44: above the guide panel's content, clear of the title bar.
  static let guideTop: CGFloat = 44
  /// 18: below the guide panel's content.
  static let guideBottom: CGFloat = 18
  /// 104: the app icon on the welcome screen.
  static let welcomeIcon: CGFloat = 104
  /// 92: the app icon on the done screen.
  static let doneIcon: CGFloat = 92
  /// 72: the app icon on the helper's allow screen.
  static let allowIcon: CGFloat = 72
  /// 40: the warning symbol on the blocked screen.
  static let blockedSymbol: CGFloat = 40
  /// 14: how far below its place a screen starts as it arrives.
  static let entryOffset: CGFloat = 14
  /// −10: how far above its place a screen ends as it leaves.
  static let exitOffset: CGFloat = -10
  /// 0.08 seconds: a screen arrives just after the last one starts to leave.
  static let entryDelay: TimeInterval = 0.08
}
