import SwiftUI

/// The helper, as Atlas's own app icon with a small status dot on its bottom-trailing corner: green while it is ready,
/// orange while it needs the person, and a hollow ring while it connects. The dot sits on the icon's rounded corner,
/// overlapping it rather than hanging off it, inside a ring of the ground behind it, a pill row's hover, press or
/// selection fill included, changing with it, so it reads cut out of the icon;
/// its tone changes on ``Motion/crossfade``. The icon is laid out by its visible tile, its file's clear margin
/// overhanging. VoiceOver skips it; the words beside it say the same. The viewer's helper line, Settings' Helper row
/// and the Helper page's card all draw it.
struct HelperMark: View {
  /// The dot's tone; quiet draws it hollow.
  let tone: StatusTone
  /// The icon's visible tile, in points.
  var tile = HelperMarkLayout.rowTile
  /// What the mark rests on, whose colour the dot's ring takes.
  var ground = Color.groundRecessed
  /// A see-through fill over that ground, such as a group's, which the ring takes too, so it matches the colour it is
  /// drawn over exactly; none by default.
  var wash = Color.clear
  /// The fill of the pill row the mark stands in, hovered, pressed or selected, which the ring takes as well.
  @Environment(\.pillWash) private var pillWash

  /// The icon with its dot.
  var body: some View {
    AppIcon(tile: tile)
      .frame(width: tile, height: tile)
      .overlay(alignment: .bottomTrailing) {
        dot
          .offset(x: HelperMarkLayout.ring, y: HelperMarkLayout.ring)
      }
      .accessibilityHidden(true)
  }

  /// The dot in the helper's tone inside its ground ring, or hollow while quiet.
  private var dot: some View {
    ZStack {
      Circle()
        .fill(ground)
      Circle()
        .fill(wash)
      Circle()
        .fill(pillWash)
      if tone == .quiet {
        Circle()
          .strokeBorder(Color.textTertiary, lineWidth: Stroke.emphasis)
          .padding(HelperMarkLayout.ring)
      } else {
        Circle()
          .fill(tone.color)
          .padding(HelperMarkLayout.ring)
      }
    }
    .frame(width: HelperMarkLayout.reach, height: HelperMarkLayout.reach)
    .animation(Motion.crossfade, value: tone)
  }
}

#if DEBUG
  #Preview("Every tone, light and dark") {
    VStack(spacing: Spacing.l) {
      ForEach([ColorScheme.light, .dark], id: \.self) { scheme in
        HStack(spacing: Spacing.l) {
          ForEach([StatusTone.ready, .attention, .quiet], id: \.self) { tone in
            HelperMark(tone: tone)
            HelperMark(tone: tone, tile: HelperMarkLayout.cardTile, ground: .fillContent, wash: .fillGroupInset)
          }
        }
        .padding(Spacing.l)
        .background(Color.groundRecessed)
        .environment(\.colorScheme, scheme)
      }
    }
  }

  #Preview("On a pill row: at rest, hovered, pressed and selected") {
    VStack(spacing: Spacing.l) {
      ForEach([ColorScheme.light, .dark], id: \.self) { scheme in
        HStack(spacing: Spacing.s) {
          ForEach([Color.clear, .fillRowHover, .fillControlOn, .fillSelection], id: \.self) { fill in
            HelperMark(tone: .ready)
              .padding(Spacing.s)
              .background(Capsule().fill(fill))
              .environment(\.pillWash, fill)
          }
        }
        .padding(Spacing.l)
        .background(Color.groundRecessed)
        .environment(\.colorScheme, scheme)
      }
    }
  }
#endif
