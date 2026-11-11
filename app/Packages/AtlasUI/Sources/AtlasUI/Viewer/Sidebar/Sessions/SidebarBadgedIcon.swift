import SwiftUI

/// A session's icon with the badge on its bottom trailing corner saying what it does now (``StateBadge``), cut out of
/// the icon by a ring in the colour of what the row rests on; while the session's work is cleaned up, a small spinner
/// stands there in the badge's place, on the same ring. A smaller icon, such as an ended session's, scales its corner
/// with it.
struct SidebarBadgedIcon: View {
  /// The host's icon.
  let icon: RowIcon
  /// The icon's side: a session's 28 by default.
  var side = ViewerLayout.sidebarIcon
  /// What the session does now, or nil for no badge.
  let badge: SessionBadge?
  /// Whether the session's work is being cleaned up now.
  var isCleaningUp = false
  /// What the row rests on, whose colour the badge's ring takes.
  let ground: Color

  /// The icon, its badge or spinner at its corner.
  var body: some View {
    SessionIcon(icon: icon, side: side)
      .overlay(alignment: .bottomTrailing) {
        corner
          .scaleEffect(scale, anchor: .bottomTrailing)
          .offset(x: ViewerLayout.badgeReach * scale, y: ViewerLayout.badgeReach * scale)
      }
  }

  /// The corner's scale: 1 on a session's icon, less on a smaller one, so the corner keeps to the icon's proportions.
  private var scale: CGFloat {
    side / ViewerLayout.sidebarIcon
  }

  /// The badge, or the spinner while cleaning up.
  @ViewBuilder private var corner: some View {
    if isCleaningUp {
      Spinner(size: ViewerLayout.badgeSpinner)
        .padding(ViewerLayout.badgeRing)
        .background(
          Circle()
            .fill(ground)
        )
    } else {
      StateBadge(badge: badge, ground: ground)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      ForEach(PreviewStates.sessionRows) { row in
        SidebarBadgedIcon(icon: row.icon, badge: row.badge, ground: Color.groundRecessed)
      }
      SidebarBadgedIcon(
        icon: SessionSamples.claudeIcon,
        badge: nil,
        isCleaningUp: true,
        ground: Color.groundRecessed
      )
      SidebarBadgedIcon(
        icon: SessionSamples.piIcon,
        side: ViewerLayout.endedIcon,
        badge: nil,
        isCleaningUp: true,
        ground: Color.groundRecessed
      )
    }
    .padding(Spacing.l)
    .background(Color.groundRecessed)
  }
#endif
