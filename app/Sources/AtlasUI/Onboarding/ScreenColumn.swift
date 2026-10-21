import SwiftUI

/// The centred column every setup screen uses. When what it holds is taller than the window, as a long account name
/// or a long problem can make it, the top scrolls and the bottom stays pinned above the footer, so the screen's button
/// and a problem are never out of sight.
struct ScreenColumn<Content: View, Bottom: View>: View {
  /// The column's widest.
  private let width: CGFloat
  /// The screen's pieces, top to bottom.
  private let content: Content
  /// The screen's button and notes, under its pieces.
  private let bottom: Bottom

  /// Creates a column with a pinned bottom.
  ///
  /// - Parameters:
  ///   - width: The column's widest.
  ///   - content: The screen's pieces, top to bottom.
  ///   - bottom: The screen's button and notes, which stay in view when the pieces scroll.
  init(
    width: CGFloat = OnboardingLayout.column,
    @ViewBuilder content: () -> Content,
    @ViewBuilder bottom: () -> Bottom
  ) {
    self.width = width
    self.content = content()
    self.bottom = bottom()
  }

  /// Everything centred in the window while it fits, else the pieces scrolling over the pinned bottom.
  var body: some View {
    ViewThatFits(in: .vertical) {
      fitting
      scrolling
    }
  }

  /// The pieces over the bottom, centred.
  private var fitting: some View {
    VStack(spacing: Spacing.xl) {
      content
      bottom
    }
    .frame(maxWidth: width)
    .padding(.horizontal, Spacing.section)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  /// The pieces scrolling from the top, the bottom pinned under them.
  private var scrolling: some View {
    VStack(spacing: .zero) {
      ScrollView(.vertical) {
        VStack(spacing: Spacing.xl) {
          content
        }
        .frame(maxWidth: width)
        .padding(.horizontal, Spacing.section)
        .frame(maxWidth: .infinity)
      }
      VStack(spacing: Spacing.xl) {
        bottom
      }
      .frame(maxWidth: width)
      .padding(.horizontal, Spacing.section)
      .padding(.vertical, Spacing.l)
    }
  }
}

extension ScreenColumn where Bottom == EmptyView {
  /// Creates a column of pieces alone.
  ///
  /// - Parameters:
  ///   - width: The column's widest.
  ///   - content: The screen's pieces, top to bottom.
  init(width: CGFloat = OnboardingLayout.column, @ViewBuilder content: () -> Content) {
    self.init(width: width, content: content) { EmptyView() }
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
