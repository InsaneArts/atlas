import SwiftUI

/// The centred column every setup screen uses. It scrolls when what it holds is taller than the window, as a long
/// account name or a long problem can make it, so nothing is cut short and the footer stays in place.
struct ScreenColumn<Content: View>: View {
  /// The column's widest.
  var width = OnboardingLayout.column
  /// The screen's pieces, top to bottom.
  @ViewBuilder let content: Content

  /// The pieces, centred in the window while they fit, else from the top, scrolling.
  var body: some View {
    GeometryReader { proxy in
      ScrollView(.vertical) {
        VStack(spacing: Spacing.xl) {
          content
        }
        .frame(maxWidth: width)
        .padding(.horizontal, Spacing.section)
        .frame(maxWidth: .infinity, minHeight: proxy.size.height)
      }
      .scrollBounceBehavior(.basedOnSize)
    }
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
