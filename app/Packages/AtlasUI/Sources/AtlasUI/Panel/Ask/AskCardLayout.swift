import SwiftUI

/// The measures of a question's card in the panel and of what the person allowed, concentric with the panel's card:
/// each sits at the panel's inset, so its corner is the panel's less that inset, and what sits inside it nests again.
enum AskCardLayout {
  /// 14: the card's corner, the panel's less its inset, so the two curves run side by side.
  static let corner = Radius.nested(in: PanelLayout.corner, inset: PanelLayout.inset)
  /// 12: the card's inset round its content.
  static let inset = Spacing.m
  /// 10: between who asks, the title, what would be sent, the answers and the scope note.
  static let gap = Spacing.sPlus
  /// 4: between the title and the task line under it.
  static let titleGap = Spacing.xs
  /// 20: the agent's mark beside who asks, as Recent's icons are.
  static let whoIcon = PanelLayout.recentIcon
  /// 8: between the agent's mark and who asks.
  static let whoGap = Spacing.s
  /// 6: the well's corner, the card's less the well's own inset, so a picture inside it nests in the card.
  static let wellCorner = Radius.nested(in: corner, inset: Spacing.s)
  /// The well's inset round a message: 8 above and below, 10 at its sides.
  static let wellInset = EdgeInsets(top: Spacing.s, leading: Spacing.sPlus, bottom: Spacing.s, trailing: Spacing.sPlus)
  /// 4: between a message's label and the message.
  static let wellGap = Spacing.xs
  /// 4: the most lines a message shows; the rest is cut short at its end.
  static let messageLines = 4
  /// 112: a picture's height in the well, enough for a button and the line around it.
  static let pictureHeight: CGFloat = 112
  /// 304: a picture's width in the well, the panel's less its inset and the card's on both sides.
  static let pictureWidth = PanelLayout.picture.width - inset * 2
  /// The well's shape, which a picture is cut to.
  static let pictureAspect = pictureWidth / pictureHeight
  /// 6: between the answers, one under another and side by side.
  static let answerGap = Spacing.xsPlus
  /// 8: what the person allowed's inset round its content.
  static let allowedInset = Spacing.s
  /// 8: between its mark, its words and End.
  static let allowedGap = Spacing.s
}
