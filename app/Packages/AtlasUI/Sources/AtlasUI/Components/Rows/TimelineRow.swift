import SwiftUI

/// A line of what was done, a log you can scan: its time in a column of even digits, its mark, what was done
/// with why it failed under it, and something at its end, such as "now" or a screenshot to open; 38 high, a
/// faint fill under the pointer, and a hairline above it from its mark's end when it follows another line
/// (``EnvironmentValues/drawsListSeparator``).
struct TimelineRow<Mark: View, Trailing: View>: View {
  /// When it started, such as "10:31:42".
  let time: String
  /// What was done.
  let title: String
  /// Why it did not work, under it in the attention colour; nil for none.
  var reason: String?
  /// The mark between the time and the words, such as the action's kind in a tile.
  @ViewBuilder let mark: Mark
  /// What stands at the line's end.
  @ViewBuilder let trailing: Trailing
  @Environment(\.drawsListSeparator) private var drawsSeparator
  @State private var isHovered = false

  /// The time, the mark, the words and the end, with the hairline above and the fill under the pointer.
  var body: some View {
    HStack(spacing: Layout.gap) {
      Text(time)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textTertiary)
        // A clock time takes no rolling digits: it names a moment, it does not count.
        .lineLimit(1)
        .minimumScaleFactor(Layout.leastTimeScale)
        .frame(width: Layout.timeWidth, alignment: .leading)
      mark
        .frame(width: Layout.markWidth)
      words
      trailing
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
    }
    .frame(minHeight: Layout.inner)
    .padding(.vertical, Spacing.xs)
    .padding(.horizontal, Layout.inset)
    .background(fill)
    .overlay(alignment: .top) {
      if drawsSeparator {
        SeparatorLine()
          .padding(.leading, Layout.separatorInset)
      }
    }
    .onHover { isHovered = $0 }
    .animation(Motion.hover(isHovered), value: isHovered)
  }

  /// The row's fill: faint under the pointer, none otherwise.
  private var fill: Color {
    isHovered ? .fillRowHover : .clear
  }

  /// What was done, and why it did not work under it.
  private var words: some View {
    VStack(alignment: .leading, spacing: .zero) {
      Text(title)
        .font(Typography.font(.body, weight: .medium))
        .foregroundStyle(Color.textPrimary)
      if let reason {
        Text(reason)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textAttention)
      }
    }
    .fixedSize(horizontal: false, vertical: true)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

/// The row's own measures for Activity entries. At file scope because `TimelineRow` is generic.
private enum Layout {
  /// 38: the row's least height, its 4-point insets included.
  static let height: CGFloat = 38
  /// 30: the least height inside its 4-point insets.
  static let inner = height - Spacing.xs * 2
  /// 72: the time's column, wide enough for "10:31:42".
  static let timeWidth: CGFloat = 72
  /// 0.85: the least the time shrinks to stay in its column, as a 12-hour clock's "10:31:42 AM" needs.
  static let leastTimeScale: CGFloat = 0.85
  /// 22: the mark's column, a small symbol tile.
  static let markWidth: CGFloat = 22
  /// 10: between the row's columns.
  static let gap = Spacing.sPlus
  /// 16: the row's inset at each side, the pane's.
  static let inset = Spacing.l
  /// 120: where the hairline above a following row starts, at its mark's end.
  static let separatorInset = inset + timeWidth + gap + markWidth
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: .zero) {
      TimelineRow(time: String("10:31:42"), title: String("Turning Show Center off in Opie")) {
        SymbolTile(icon: .symbol(SymbolName.click.rawValue), size: .small, tone: .quiet)
      } trailing: {
        Text(verbatim: "now")
      }
      TimelineRow(
        time: String("10:28:30"),
        title: String("Couldn’t click Show Centre in Opie"),
        reason: String("No element is named Show Centre")
      ) {
        SymbolTile(icon: .symbol(SymbolName.problem.rawValue), size: .small, tone: .attention)
      } trailing: {
        EmptyView()
      }
      .environment(\.drawsListSeparator, true)
    }
    .frame(width: SampleWidth.column * 2)
    .sampleSurface()
  }
#endif
