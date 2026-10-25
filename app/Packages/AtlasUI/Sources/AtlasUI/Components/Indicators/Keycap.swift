import SwiftUI

/// A key on the keyboard, drawn as a small cap holding its symbol or letter, so a shortcut reads as keys: ⌃ ⇧
/// ⎋ gives control back.
struct Keycap: View {
  /// What the cap shows.
  enum Face: Equatable, Sendable {
    /// An SF Symbol.
    case symbol(SymbolName)
    /// A letter or a key's sign, such as ⌃ or ⎋.
    case text(String)
  }

  /// What the cap shows.
  let face: Face

  /// The face on its cap.
  var body: some View {
    label
      .font(Typography.font(.caption, weight: .medium))
      .foregroundStyle(Color.textPrimary)
      .padding(.horizontal, Spacing.xs)
      .frame(minWidth: Layout.side, minHeight: Layout.side)
      .background(
        RoundedRectangle(cornerRadius: Layout.radius)
          .filledOutline(Color.fillControlRaised, border: Color.strokeStrong)
      )
      .overlay(alignment: .bottom) {
        RoundedRectangle(cornerRadius: Layout.radius)
          .strokeBorder(Color.strokeShade, lineWidth: Stroke.regular)
          .mask(alignment: .bottom) {
            Rectangle()
              .frame(height: Stroke.regular)
          }
      }
  }

  /// The symbol or the text.
  @ViewBuilder private var label: some View {
    switch face {
    case .symbol(let name):
      Image(systemName: name.rawValue)
    case .text(let text):
      Text(text)
    }
  }
}

/// The keycap's own measures for its label and edge.
private enum Layout {
  /// 18: the cap's height, and its least width.
  static let side: CGFloat = 18
  /// 5: the cap's corner.
  static let radius: CGFloat = 5
}

#if DEBUG
  #Preview("Samples") {
    GlassChipSamples(.keycaps)
  }
#endif
