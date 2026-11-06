import SwiftUI

/// The keys pressed, as white keycaps on a dark bubble, each chord apart, with the key's name after them where it has
/// one; a key that did not go through rests on an orange edge.
struct MarkKeys: View {
  /// One chord's caps each, such as `[["⌘", "S"]]`.
  let keys: [[String]]
  /// The key's name, such as "Return"; empty for none.
  var name = ""
  /// How the action went.
  let tone: ViewerMarkTone

  /// The chords on the bubble.
  var body: some View {
    HStack(spacing: Spacing.s) {
      ForEach(keys.indices, id: \.self) { chord in
        HStack(spacing: Spacing.xs) {
          ForEach(keys[chord].indices, id: \.self) { cap in
            self.cap(keys[chord][cap])
          }
        }
      }
      if !name.isEmpty {
        Text(verbatim: name)
          .font(Typography.font(.body, weight: .semibold))
          .foregroundStyle(Color.markBubbleText)
          .padding(.trailing, Spacing.xs)
      }
    }
    .padding(Spacing.xsPlus)
    .background(
      RoundedRectangle(cornerRadius: Radius.medium, style: .continuous)
        .fill(Color.markBubble)
    )
    .shadow(color: Color.dropShadow, radius: Spacing.xsPlus, y: Stroke.regular)
    .fixedSize()
  }

  /// One keycap, on its lower edge.
  ///
  /// - Parameter key: Its letter or symbol.
  /// - Returns: The cap.
  private func cap(_ key: String) -> some View {
    Text(verbatim: key)
      .font(Typography.font(.body, weight: .semibold))
      .foregroundStyle(Color.markKeyText)
      .padding(.horizontal, Spacing.xsPlus)
      .padding(.vertical, Spacing.half)
      .background(
        RoundedRectangle(cornerRadius: Radius.extraSmall, style: .continuous)
          .fill(Color.markKey)
      )
      .background(
        RoundedRectangle(cornerRadius: Radius.extraSmall, style: .continuous)
          .fill(tone == .worked ? Color.markKeyEdge : tone.color)
          .offset(y: Stroke.thick)
      )
  }
}
