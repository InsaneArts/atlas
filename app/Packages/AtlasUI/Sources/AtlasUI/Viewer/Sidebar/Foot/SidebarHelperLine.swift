import SwiftUI

/// The helper's health, last in the sidebar, as one of its rows: the same height, insets, capsule and hover as a
/// session's, with Atlas's mark and its dot and a title over a line, such as "Atlas is ready" over "Helper atlas ·
/// 1440 × 900". A click anywhere on it opens Settings on Helper (``click``); a right-click opens the helper's menu:
/// every fact about the helper, read-only, then Check Again, Open Settings on Helper…, Recover…, Switch… and Log Out….
/// In a short window the mark and its dot stand alone in a circle that does the same. VoiceOver reads the title and the
/// line, and hears that the row opens Settings.
struct SidebarHelperLine: View {
  /// What a click sends: open Settings on Helper.
  static let click = ViewerEvent.menu(.openSettings)

  /// The helper's health.
  let helper: ViewerHelperLine
  /// Whether the mark stands alone in a circle, as a short window leaves it.
  var isCompact = false
  /// Receives the click, or the command chosen from the menu.
  let send: @MainActor (ViewerEvent) -> Void

  /// The row, or the circle, opening Settings when clicked and the helper's menu on a right-click.
  @ViewBuilder var body: some View {
    if isCompact {
      button {
        SidebarHelperMark(tone: helper.tone)
          .frame(width: ViewerLayout.helperCircle, height: ViewerLayout.helperCircle)
      }
      .fixedSize()
      .padding(.leading, ViewerLayout.helperCircleInset)
    } else {
      button {
        SidebarHelperLabel(helper: helper)
          .frame(width: ViewerLayout.rowContentWidth, alignment: .leading)
          .padding(.horizontal, ViewerLayout.sidebarRowInset)
          .frame(minHeight: ViewerLayout.sidebarRowHeight)
      }
    }
  }

  /// `label` as a button that opens Settings, with the helper's menu as its context menu.
  ///
  /// - Parameter label: What a click opens Settings from.
  /// - Returns: The button.
  private func button(@ViewBuilder label: () -> some View) -> some View {
    Button {
      send(Self.click)
    } label: {
      label()
    }
    .buttonStyle(.pill)
    .contextMenu {
      CommandMenu(sections: helper.menu) { send(.menu($0)) }
    }
    .help(Text(verbatim: "Open Settings on Helper"))
    .accessibilityLabel(Text(verbatim: helper.line.isEmpty ? helper.title : "\(helper.title), \(helper.line)"))
    .accessibilityHint(Text(verbatim: "Opens Settings"))
  }
}

#if DEBUG
  #Preview("Every state") {
    HelperRowSamples()
  }
#endif
