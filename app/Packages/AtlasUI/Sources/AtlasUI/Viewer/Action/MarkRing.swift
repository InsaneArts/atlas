import SwiftUI

/// A mark's ring, a fixed size centred on the spot, where no element's frame is known: a line in the mark's colour,
/// dashed when the action did not work or may not have finished, a fine white halo inside it and a soft glow outside,
/// so it reads over any screen.
struct MarkRing: View {
  /// How the action went.
  let tone: ViewerMarkTone
  /// How large it draws.
  let style: ActionMarkStyle

  /// The glow, the line and the halo.
  var body: some View {
    Circle()
      .strokeBorder(
        tone.color,
        style: StrokeStyle(lineWidth: style.line, dash: tone.isDashed ? ViewerLayout.markDash : [])
      )
      .background(
        Circle()
          .strokeBorder(Color.markHalo, lineWidth: Stroke.hairline)
          .padding(style.line)
      )
      .background(
        Circle()
          .stroke(tone.glow, lineWidth: style.glow)
          .padding(-style.glow / 2)
      )
      .frame(width: style.ring, height: style.ring)
  }
}
