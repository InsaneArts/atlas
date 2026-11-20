import SwiftUI

/// The helper's health, last in the sidebar, as one of its rows: the same height, insets, capsule and hover as a
/// session's, with Atlas's mark and its dot and a title over a line, such as "Atlas is ready" over "Helper atlas ·
/// 1440 × 900". A click anywhere on it opens Settings on Helper (``click``), or, while more than one desktop is set up,
/// the desktops in a menu over it, to pick the one Atlas shows; a right-click opens the helper's menu: every fact about
/// the helper, read-only, then Check Again, Open Settings on Helper…, Recover…, Switch… and Log Out…. In a short window
/// the mark and its dot stand alone in a circle that does the same. VoiceOver reads the title and the line, and hears
/// what a click opens.
struct SidebarHelperLine: View {
  /// What a click sends: open Settings on Helper.
  static let click = ViewerEvent.menu(.openSettings)

  /// The helper's health.
  let helper: ViewerHelperLine
  /// Whether the mark stands alone in a circle, as a short window leaves it.
  var isCompact = false
  /// Receives the click, or the command chosen from the menu.
  let send: @MainActor (ViewerEvent) -> Void
  /// Where the row is in the window, so the desktops' menu opens over it.
  @State private var frame = CGRect.zero

  /// The row, or the circle, opening Settings when clicked and the helper's menu on a right-click.
  @ViewBuilder var body: some View {
    if isCompact {
      button {
        HelperMark(tone: helper.tone)
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

  /// The sections its right-click menu lists: every fact about the helper, then its commands, as the window's
  /// helper menu has them.
  ///
  /// - Parameter helper: The helper's health.
  /// - Returns: The sections.
  static func contextMenu(_ helper: ViewerHelperLine) -> [ViewerMenuSection] {
    helper.menu
  }

  /// What a click sends: the desktops over the row while it picks one, else Settings on Helper.
  ///
  /// - Parameters:
  ///   - helper: The helper's health.
  ///   - frame: The row's frame in the window.
  /// - Returns: The event.
  static func clicked(_ helper: ViewerHelperLine, frame: CGRect) -> ViewerEvent {
    helper.picksDesktop ? .showDesktops(from: frame) : click
  }

  /// `label` as a button that opens Settings or the desktops, with the helper's menu as its context menu.
  ///
  /// - Parameter label: What a click opens Settings or the desktops from.
  /// - Returns: The button.
  private func button(@ViewBuilder label: () -> some View) -> some View {
    Button {
      send(Self.clicked(helper, frame: frame))
    } label: {
      label()
    }
    .buttonStyle(.pill)
    .onGeometryChange(for: CGRect.self) {
      $0.frame(in: .global)
    } action: {
      frame = $0
    }
    .contextMenu {
      CommandMenu(sections: Self.contextMenu(helper)) { send(.menu($0)) }
    }
    .help(Text(verbatim: helper.picksDesktop ? "Choose a desktop" : "Open Settings on Helper"))
    .accessibilityLabel(Text(verbatim: helper.line.isEmpty ? helper.title : "\(helper.title), \(helper.line)"))
    .accessibilityHint(Text(verbatim: helper.picksDesktop ? "Opens a menu" : "Opens Settings"))
  }
}

#if DEBUG
  #Preview("Every state") {
    HelperRowSamples()
  }
#endif
