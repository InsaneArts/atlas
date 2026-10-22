import SwiftUI

/// A line of what was done, over a hairline: its time, its mark, what was done (in bold while it runs) with why it
/// failed under it, and something at its end, such as "now" or a screenshot to open.
struct TimelineRow<Mark: View, Trailing: View>: View {
  /// When it started, such as "10:31:42".
  let time: String
  /// What was done.
  let title: String
  /// Why it did not work, under it in the attention colour; nil for none.
  var reason: String?
  /// Whether it is under way now, which draws its words bold.
  var isLive = false
  /// The mark between the time and the words.
  @ViewBuilder let mark: Mark
  /// What stands at the line's end.
  @ViewBuilder let trailing: Trailing

  var body: some View {
    HStack(spacing: Spacing.l) {
      Text(time)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(Layout.leastTimeScale)
        .frame(width: Layout.timeWidth, alignment: .leading)
      mark
        .frame(width: Layout.markWidth)
      VStack(alignment: .leading, spacing: Spacing.xxs) {
        Text(title)
          .font(Typography.font(.body, weight: isLive ? .semibold : .regular))
          .foregroundStyle(Color.textPrimary)
        if let reason {
          Text(reason)
            .font(Typography.font(.callout))
            .foregroundStyle(Color.textAttention)
        }
      }
      .fixedSize(horizontal: false, vertical: true)
      .frame(maxWidth: .infinity, alignment: .leading)
      trailing
    }
    .frame(minHeight: ControlHeight.large)
    .padding(.vertical, Spacing.xs)
    .padding(.horizontal, Layout.inset)
    .overlay(alignment: .top) {
      SeparatorLine()
    }
  }
}

/// At file scope because `TimelineRow` is generic.
private enum Layout {
  /// 70: the time's column, the board's (mac.py `board_activity`), wide enough for "10:31:42".
  static let timeWidth: CGFloat = 70
  /// 0.85: the least the time shrinks to stay in its column, as a 12-hour clock's "10:31:42 AM" needs.
  static let leastTimeScale: CGFloat = 0.85
  /// 16: the mark's column, the board's.
  static let markWidth: CGFloat = 16
  /// 18: the pane's text inset, the board's (mac.py `head`), which the pane header keeps too, between the grid's 16
  /// and 20.
  static let inset: CGFloat = 18
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: .zero) {
      TimelineRow(time: String("10:31:42"), title: String("Turning Show Center off in Opie"), isLive: true) {
        ActivityMark(kind: .live)
      } trailing: {
        Text(verbatim: "now")
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
      }
      TimelineRow(
        time: String("10:28:30"),
        title: String("Couldn’t click Show Centre in Opie"),
        reason: String("No element is named Show Centre")
      ) {
        ActivityMark(kind: .failed)
      } trailing: {
        EmptyView()
      }
    }
    .frame(width: SampleWidth.column * 2)
    .sampleSurface()
  }
#endif
