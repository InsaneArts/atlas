import SwiftUI

/// A card's ground, and a tinted card's edge: neutral, read by its fill alone, tinted blue for the one chosen or
/// current, or orange for what needs the person.
struct CardSurface: ViewModifier {
  /// The card's tint.
  enum Tint: Sendable {
    /// The quiet neutral card.
    case neutral
    /// A blue wash with a firmer edge: the row chosen, or the step to do now.
    case accent
    /// An orange wash: something needs the person.
    case attention
  }

  /// The card's tint.
  let tint: Tint
  /// The card's corners: `Radius.large` for a card on its own, `Radius.medium` for a group of rows in a column.
  let radius: CGFloat

  /// Lays the card's ground under the content and, on a tinted card, its edge over it.
  ///
  /// - Parameter content: The card's content.
  /// - Returns: The content on its card.
  func body(content: Content) -> some View {
    content
      .background(
        RoundedRectangle(cornerRadius: radius, style: .continuous)
          .fill(fill)
      )
      .overlay(
        RoundedRectangle(cornerRadius: radius, style: .continuous)
          .strokeBorder(edge, lineWidth: tint == .accent ? Stroke.regular : Stroke.hairline)
      )
  }

  private var fill: Color {
    switch tint {
    case .neutral: .fillCard
    case .accent:
      Color.accent
        .opacity(Opacity.tint)
    case .attention:
      Color.attention
        .opacity(Opacity.tint)
    }
  }

  private var edge: Color {
    switch tint {
    case .neutral: .clear
    case .accent:
      Color.accent
        .opacity(Opacity.edgeTint)
    case .attention:
      Color.attention
        .opacity(Opacity.edgeTintQuiet)
    }
  }
}

#if DEBUG
  #Preview("Tints") {
    ModifierSamples(.cardSurface)
  }
#endif
