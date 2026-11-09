import SwiftUI

/// Atlas's orbit on its white tile, the size of the plate a session's app icon draws, with a dot in the helper's tone
/// where a session's badge stands, cut out of the tile by a ring in the ground's colour. The tile keeps a
/// hairline edge on the light ground, and the orbit dims while the window connects. VoiceOver skips it; the row's words
/// say the same.
struct SidebarHelperMark: View {
  /// The dot's tone; quiet dims the mark.
  let tone: StatusTone
  /// What the row rests on, whose colour the dot's ring takes.
  var ground = Color.groundRecessed

  /// The tile, its orbit and the dot.
  var body: some View {
    tile
      .frame(width: ViewerLayout.sidebarIcon, height: ViewerLayout.sidebarIcon)
      .overlay(alignment: .bottomTrailing) {
        dot
          .offset(x: ViewerLayout.badgeReach, y: ViewerLayout.badgeReach)
      }
      .accessibilityHidden(true)
  }

  /// The white tile with the graphite orbit, the orbit dimmed while the window connects.
  private var tile: some View {
    OrbitMark(size: ViewerLayout.helperMark)
      .foregroundStyle(Color.brandAtlasInk)
      .opacity(tone == .quiet ? Opacity.muted : 1)
      .frame(width: ViewerLayout.helperTile, height: ViewerLayout.helperTile)
      .background {
        RoundedRectangle(cornerRadius: ViewerLayout.helperTileCorner, style: .continuous)
          .fill(Color.brandWhite)
      }
      .overlay {
        RoundedRectangle(cornerRadius: ViewerLayout.helperTileCorner, style: .continuous)
          .strokeBorder(Color.brandAtlasEdge, lineWidth: Stroke.hairline)
      }
  }

  /// The dot in the helper's tone inside its ground ring, as a session's badge stands.
  private var dot: some View {
    Circle()
      .fill(tone.color)
      .frame(width: StateBadgeLayout.side, height: StateBadgeLayout.side)
      .padding(StateBadgeLayout.ring)
      .background(
        Circle()
          .fill(ground)
      )
      .frame(width: StateBadgeLayout.reach, height: StateBadgeLayout.reach)
      .animation(Motion.crossfade, value: tone)
  }
}

#if DEBUG
  #Preview("Every tone") {
    HStack(spacing: Spacing.l) {
      ForEach([StatusTone.ready, .attention, .quiet], id: \.self) { tone in
        SidebarHelperMark(tone: tone)
      }
    }
    .padding(Spacing.l)
    .background(Color.groundRecessed)
  }
#endif
