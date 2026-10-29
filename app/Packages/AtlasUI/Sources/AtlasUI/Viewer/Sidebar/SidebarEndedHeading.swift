import SwiftUI

/// Ended's heading: a chevron, the word and how many sessions ended, one button that opens Ended, the chevron turning
/// down as the rows come in from under it, and folds it again; a faint fill under the pointer. Under Reduce Motion the
/// chevron does not turn: one glyph crossfades into the other.
struct SidebarEndedHeading: View {
  /// How many sessions ended.
  let count: Int
  /// Whether Ended is open.
  let isOpen: Bool
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The heading as a button.
  var body: some View {
    Button {
      send(.toggleEnded)
    } label: {
      HStack(spacing: ViewerLayout.headingChevronGap) {
        chevron
        Text(verbatim: ViewerSessionSection.ended.title)
          .frame(maxWidth: .infinity, alignment: .leading)
        Text(count, format: .number)
          .fontWeight(.medium)
          .monospacedDigit()
      }
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(Color.textTertiary)
      .lineLimit(1)
      .padding(.horizontal, ViewerLayout.headingInset.leading)
      .frame(width: ViewerLayout.pillWidth, height: ViewerLayout.endedHeadingHeight)
    }
    .buttonStyle(.row(corner: ViewerLayout.endedHeadingCorner))
    .help(Text(verbatim: isOpen ? "Hide Ended Sessions" : "Show Ended Sessions"))
    .accessibilityLabel(Text(verbatim: "Ended, \(count)"))
    .accessibilityValue(Text(verbatim: isOpen ? "Shown" : "Hidden"))
    .padding(ViewerLayout.endedHeadingInset)
    .animation(ViewerLayout.endedMotion(reduceMotion: reduceMotion), value: isOpen)
  }

  /// The chevron: pointing on while folded, turning down as Ended opens; under Reduce Motion, the one pointing down
  /// crossfading in.
  private var chevron: some View {
    let symbol = reduceMotion && isOpen ? SymbolName.menu : SymbolName.next
    return Image(systemName: symbol.rawValue)
      .font(Typography.font(.caption2, weight: .bold))
      .frame(width: ViewerLayout.headingChevron)
      .rotationEffect(.degrees(!reduceMotion && isOpen ? ViewerLayout.chevronTurn : .zero))
      .contentTransition(.opacity)
      .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isOpen = false
    InteractivePreview {
      Text(verbatim: isOpen ? "Open" : "Folded")
    } content: {
      SidebarEndedHeading(count: 3, isOpen: isOpen) { _ in isOpen.toggle() }
        .frame(width: ViewerLayout.sidebarWidth)
        .background(Color.groundRecessed)
    }
  }
#endif
