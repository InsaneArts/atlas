import SwiftUI

/// The centred column every setup screen uses.
struct ScreenColumn<Content: View>: View {
  /// The column's widest.
  var width = OnboardingLayout.column
  /// The screen's pieces, top to bottom.
  @ViewBuilder let content: Content

  /// The pieces, centred in the window.
  var body: some View {
    VStack(spacing: Spacing.xl) {
      content
    }
    .frame(maxWidth: width)
    .padding(.horizontal, Spacing.section)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}

#if DEBUG
  #Preview("A title over a lede") {
    ScreenColumn {
      Eyebrow(text: String("Helper account"))
      Headline(title: String("Agents need an account"), tail: String("of their own."))
      Lede(text: String("A standard account on this Mac that only Atlas uses."))
    }
    .onboardingSample()
  }
#endif
