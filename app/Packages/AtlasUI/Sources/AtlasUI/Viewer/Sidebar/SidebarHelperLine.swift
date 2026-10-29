import SwiftUI

/// The helper's health, last in the sidebar: a dot in its tone and a few words, such as "Helper ready · atlas", every
/// fact about the helper in its tooltip. A click opens the helper's commands: Check Again, Open Settings on Helper…,
/// Recover…, Switch… and Log Out…; a faint capsule fills under the pointer. At the end of the Library folded into a
/// row, the dot stands alone in a circle. VoiceOver reads the words, and offers the menu as a pop-up button's.
struct SidebarHelperLine: View {
  /// The helper's health.
  let helper: ViewerHelperLine
  /// Whether the dot stands alone in a circle, at the end of the Library folded into a row.
  var isCompact = false
  /// Receives the command chosen.
  let send: @MainActor (ViewerEvent) -> Void

  /// The line, opening its menu when clicked.
  var body: some View {
    Menu {
      CommandMenu(sections: helper.menu) { send(.menu($0)) }
    } label: {
      line
    }
    .menuStyle(.button)
    .menuIndicator(.hidden)
    .buttonStyle(.row(corner: isCompact ? ViewerLayout.libraryCircle / 2 : ViewerLayout.helperCorner))
    .fixedSize()
    .help(Text(verbatim: helper.help.isEmpty ? helper.words : "\(helper.words)\n\(helper.help)"))
    .accessibilityLabel(Text(helper.words))
  }

  /// The dot and the words, the width of a row's pill; or the dot alone in its circle.
  @ViewBuilder private var line: some View {
    if isCompact {
      dot
        .frame(width: ViewerLayout.libraryCircle, height: ViewerLayout.libraryCircle)
        .contentShape(Circle())
    } else {
      words
    }
  }

  /// The dot in the helper's tone.
  private var dot: some View {
    Circle()
      .fill(helper.tone.color)
      .frame(width: ViewerLayout.helperDot, height: ViewerLayout.helperDot)
      .accessibilityHidden(true)
  }

  /// The dot and the words, the width of a row's pill.
  private var words: some View {
    HStack(spacing: ViewerLayout.helperGap) {
      dot
      Text(helper.words)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textSecondary)
        .lineLimit(1)
        .truncationMode(.tail)
    }
    .frame(width: ViewerLayout.rowContentWidth, alignment: .leading)
    .padding(.horizontal, ViewerLayout.sidebarRowInset)
    .padding(.vertical, ViewerLayout.helperInset)
    .contentShape(Rectangle())
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.s) {
      SidebarHelperLine(helper: PreviewStates.viewerHelperReady(logsOut: true)) { _ in }
      SidebarHelperLine(helper: PreviewStates.viewerHelperSetup) { _ in }
    }
    .padding(.horizontal, ViewerLayout.sidebarInset)
    .frame(width: ViewerLayout.sidebarWidth)
    .background(Color.groundRecessed)
  }
#endif
