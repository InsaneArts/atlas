import SwiftUI

/// A Settings page's sections in a column at the page's insets, scrolling when the card is shorter than they are, and
/// fading out over ``SettingsLayout/pageFade`` under the bar and at the card's foot. The page's own insets are as deep
/// as the fades, or deeper, so at either end of its scroll its first and last controls stand whole.
struct SettingsPageScroll<Content: View>: View {
  /// The sections.
  private let content: Content

  /// Creates a page's column.
  ///
  /// - Parameter content: The sections.
  init(@ViewBuilder content: () -> Content) {
    self.content = content()
  }

  /// The content in its scroll view, faded at both ends.
  var body: some View {
    ScrollView {
      content
        .padding(SettingsLayout.pageInsets)
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }
    .scrollIndicators(.automatic)
    .scrollBounceBehavior(.basedOnSize)
    .mask { fade }
  }

  /// Whole between its ends, clear at its very edges.
  private var fade: some View {
    VStack(spacing: .zero) {
      LinearGradient(colors: [.clear, .black], startPoint: .top, endPoint: .bottom)
        .frame(height: SettingsLayout.pageFade)
      Rectangle()
      LinearGradient(colors: [.black, .clear], startPoint: .top, endPoint: .bottom)
        .frame(height: SettingsLayout.pageFade)
    }
  }
}
