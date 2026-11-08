import SwiftUI

/// A mark's ring around the element an action went to, filling the frame it is given: a rounded line in the mark's
/// colour, dashed when the action did not work or may not have finished, a fine white halo inside it and a soft band
/// outside, so it reads over any screen.
struct MarkOutline: View {
  /// How the action went.
  let tone: ViewerMarkTone
  /// How large it draws.
  let style: ActionMarkStyle

  /// The band, the line and the halo.
  var body: some View {
    RoundedRectangle(cornerRadius: style.outlineRadius, style: .continuous)
      .strokeBorder(
        tone.color,
        style: StrokeStyle(lineWidth: style.line, dash: tone.isDashed ? ViewerLayout.markDash : [])
      )
      .background(
        RoundedRectangle(
          cornerRadius: Radius.nested(in: style.outlineRadius, inset: style.line),
          style: .continuous
        )
        .strokeBorder(Color.markHalo, lineWidth: Stroke.hairline)
        .padding(style.line)
      )
      .background(
        RoundedRectangle(cornerRadius: style.outlineRadius + style.glow / 2, style: .continuous)
          .stroke(tone.glow, lineWidth: style.glow)
          .padding(-style.glow / 2)
      )
  }
}
