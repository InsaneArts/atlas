import SwiftUI

/// The helper's health, last in the sidebar, as one of its rows: the same height, insets, capsule and hover as a
/// session's, with Atlas's mark and its dot and a title over a line, such as "Atlas is ready" over "Helper atlas ·
/// 1440 × 900". A click anywhere on it opens the helper's menu, the row held pressed while the menu is open: every
/// fact about the helper, read-only, then Check Again, Open Settings on Helper…, Recover…, Switch… and Log Out…. In a
/// short window the mark and its dot stand alone in a circle that opens the same menu. VoiceOver reads the title and
/// the line, and offers the menu as a pop-up button's.
struct SidebarHelperLine: View {
  /// The helper's health.
  let helper: ViewerHelperLine
  /// Whether the mark stands alone in a circle, as a short window leaves it.
  var isCompact = false
  /// Receives the command chosen.
  let send: @MainActor (ViewerEvent) -> Void

  /// The row, or the circle, opening its menu when clicked.
  @ViewBuilder var body: some View {
    if isCompact {
      menu {
        SidebarHelperMark(tone: helper.tone)
          .frame(width: ViewerLayout.helperCircle, height: ViewerLayout.helperCircle)
          .contentShape(Circle())
      }
      .buttonStyle(.row(corner: ViewerLayout.helperCircle / 2))
      .fixedSize()
      .padding(.leading, ViewerLayout.helperCircleInset)
    } else {
      menu {
        SidebarHelperLabel(helper: helper)
      }
      .buttonStyle(SidebarRowStyle(isSelected: false, height: ViewerLayout.sidebarRowHeight))
    }
  }

  /// The helper's menu behind `label`.
  ///
  /// - Parameter label: What a click opens the menu from.
  /// - Returns: The menu.
  private func menu(@ViewBuilder label: () -> some View) -> some View {
    Menu {
      CommandMenu(sections: helper.menu) { send(.menu($0)) }
    } label: {
      label()
    }
    .menuStyle(.button)
    .menuIndicator(.hidden)
    .accessibilityLabel(Text(verbatim: helper.line.isEmpty ? helper.title : "\(helper.title), \(helper.line)"))
  }
}

#if DEBUG
  #Preview("Every state") {
    HelperRowSamples()
  }
#endif
