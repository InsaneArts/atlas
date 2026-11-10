import SwiftUI

/// The dot where the pointer landed: the mark's colour inside a white halo and a soft glow.
struct MarkDot: View {
  /// How the action went.
  let tone: ViewerMarkTone
  /// How large it draws.
  let style: ActionMarkStyle

  /// The dot, its halo and its glow.
  var body: some View {
    Circle()
      .fill(tone.color)
      .padding(Stroke.thick)
      .background(Circle().fill(Color.markHalo))
      .background(
        Circle()
          .fill(tone.glow)
          .padding(-style.glow)
      )
      .frame(width: style.dot + Stroke.thick * 2, height: style.dot + Stroke.thick * 2)
  }
}
