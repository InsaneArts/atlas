import SwiftUI

extension View {
  /// Sets the view on a card.
  ///
  /// - Parameters:
  ///   - tint: The card's tint; neutral by default.
  ///   - radius: The card's corners: `Radius.large` for a card on its own, by default, or `Radius.medium` for a
  ///     group of rows in a column.
  /// - Returns: The view on its card.
  func cardSurface(_ tint: CardSurface.Tint = .neutral, radius: CGFloat = Radius.large) -> some View {
    modifier(CardSurface(tint: tint, radius: radius))
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.m) {
      TitleDetail(title: String("A neutral card"), detail: String("Facts and settings sit on it."))
        .padding(Spacing.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface()
      TitleDetail(title: String("An accent card"), detail: String("The row chosen, or the step to do now."))
        .padding(Spacing.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface(.accent)
      TitleDetail(title: String("An attention card"), detail: String("Something needs the person."))
        .padding(Spacing.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface(.attention)
    }
    .sampleSurface()
  }
#endif
