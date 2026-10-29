import SwiftUI

/// All sessions' icons: the first live sessions' as a small deck, the first standing whole in front and each after it
/// tucked behind, its edge peeking out on the trailing side, with a soft shadow from the one in front telling them
/// apart and nothing drawn behind them; while none is live, a stack's symbol in grey, so the row is never blank.
struct SidebarIconStack: View {
  /// The icons, at most three, the first in front.
  let icons: [RowIcon]

  /// The deck, from the leading edge.
  var body: some View {
    ZStack(alignment: .leading) {
      if icons.isEmpty {
        Image(systemName: SymbolName.allSessions.rawValue)
          .font(Typography.symbol(.glyph))
          .foregroundStyle(Color.textSecondary)
          .frame(width: ViewerLayout.sidebarIcon, height: ViewerLayout.sidebarIcon)
      }
      ForEach(Array(icons.enumerated()), id: \.offset) { index, icon in
        SessionIcon(icon: icon, side: ViewerLayout.stackIcon)
          .shadow(
            color: Color.dropShadowSoft,
            radius: ViewerLayout.stackShadowRadius,
            x: ViewerLayout.stackShadowOffset
          )
          .offset(x: ViewerLayout.stackStep * CGFloat(index))
          .zIndex(Double(icons.count - index))
      }
    }
    .frame(
      width: ViewerLayout.stackWidth(count: icons.count),
      height: ViewerLayout.sidebarIcon,
      alignment: .leading
    )
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Samples") {
    SidebarIconStack(icons: PreviewStates.allSessions.icons)
      .padding(Spacing.l)
      .background(Color.groundRecessed)
  }
#endif
