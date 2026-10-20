import SwiftUI

extension View {
  /// Sets the view on a card.
  ///
  /// - Parameter tint: The card's tint; neutral by default.
  /// - Returns: The view on its card.
  func cardSurface(_ tint: CardSurface.Tint = .neutral) -> some View {
    modifier(CardSurface(tint: tint))
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.l) {
      TitleDetail(title: String("A neutral card"), detail: String("Facts and settings sit on it."))
        .padding(Spacing.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface()
      TitleDetail(title: String("An accent card"), detail: String("The row chosen, or the step to do now."))
        .padding(Spacing.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface(.accent)
      TitleDetail(title: String("An attention card"), detail: String("Something needs the person."))
        .padding(Spacing.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface(.attention)
    }
    .sampleSurface()
  }
#endif
