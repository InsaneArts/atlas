import SwiftUI

/// A short tag on a mark, in its colour, such as why it failed or "Down ×12".
struct MarkTag: View {
  /// What it says.
  let text: String
  /// How the action went, which its colour shows.
  let tone: ViewerMarkTone

  /// The words on a capsule.
  var body: some View {
    Text(verbatim: text)
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(Color.onMark)
      .lineLimit(1)
      .padding(.horizontal, Spacing.s)
      .padding(.vertical, Spacing.half)
      .background(Capsule().fill(tone.color))
      .shadow(color: Color.dropShadowSoft, radius: Spacing.half, y: Stroke.regular)
      .fixedSize()
  }
}
