import SwiftUI

/// A centred column of one width, its pieces stacked with no gaps of their own: each piece brings its own space above
/// it, as its window's rhythm says. While everything fits, the column sits in the middle; when it is taller than its
/// room, as a long account name or a long problem can make it, the pieces scroll under a 24-point fade (v3 D9) and the
/// bottom stays pinned under them, so a problem is never out of sight.
struct ScrollColumn<Content: View, Bottom: View>: View {
  /// The column's widest.
  private let width: CGFloat
  /// The pieces, top to bottom.
  private let content: Content
  /// The pieces that stay in view when the rest scrolls: the status and the notes under it.
  private let bottom: Bottom

  /// Creates a column with a pinned bottom.
  ///
  /// - Parameters:
  ///   - width: The column's widest: one width for every page of a window, so its edges never shift.
  ///   - content: The pieces, top to bottom.
  ///   - bottom: The pieces that stay in view when the rest scrolls.
  init(width: CGFloat, @ViewBuilder content: () -> Content, @ViewBuilder bottom: () -> Bottom) {
    self.width = width
    self.content = content()
    self.bottom = bottom()
  }

  /// Everything centred while it fits, else the pieces scrolling over the pinned bottom.
  var body: some View {
    ViewThatFits(in: .vertical) {
      fitting
      scrolling
    }
  }

  /// The pieces over the bottom, centred.
  private var fitting: some View {
    VStack(spacing: .zero) {
      content
      bottom
    }
    .frame(maxWidth: width)
    .padding(.horizontal, Spacing.xxl)
    .padding(.top, Spacing.l)
    .padding(.bottom, Spacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  /// The pieces scrolling from the top, the bottom pinned under them.
  private var scrolling: some View {
    VStack(spacing: .zero) {
      ScrollView(.vertical) {
        VStack(spacing: .zero) {
          content
        }
        .frame(maxWidth: width)
        .padding(.horizontal, Spacing.xxl)
        .padding(.top, Spacing.l)
        // Room for the fade below the last piece, so scrolled to the end it is read in full.
        .padding(.bottom, Layout.fade)
        .frame(maxWidth: .infinity)
      }
      .scrollIndicators(.never)
      .mask { fade }
      VStack(spacing: .zero) {
        bottom
      }
      .frame(maxWidth: width)
      .padding(.horizontal, Spacing.xxl)
      .padding(.bottom, Spacing.xl)
    }
  }
}

extension ScrollColumn {
  /// Opaque down to its last ``Layout/fade`` points, which fade out: scrolled content ends in a fade, never on a clean
  /// line that reads as finished, and no rule is drawn above what follows (v3 D9). A mask reads only alpha: the filled
  /// rectangle shows the content whole, and the gradient's black to clear fades it.
  fileprivate var fade: some View {
    VStack(spacing: .zero) {
      Rectangle()
      LinearGradient(
        colors: [.black, .clear],
        startPoint: .top,
        endPoint: .bottom
      )
      .frame(height: Layout.fade)
    }
  }
}

extension ScrollColumn where Bottom == EmptyView {
  /// Creates a column of pieces alone.
  ///
  /// - Parameters:
  ///   - width: The column's widest: one width for every page of a window, so its edges never shift.
  ///   - content: The pieces, top to bottom.
  init(width: CGFloat, @ViewBuilder content: () -> Content) {
    self.init(width: width, content: content) { EmptyView() }
  }
}

/// v3's D9 fade.
private enum Layout {
  /// 24: how far scrolling content fades out above what is pinned under it.
  static let fade: CGFloat = 24
}

#if DEBUG
  #Preview("A title over a lede") {
    ScrollColumn(width: SampleWidth.column) {
      GroupLabel(text: String("Helper account"))
      Headline(title: String("Agents need an account"), tail: String("of their own."))
        .padding(.top, Spacing.xsPlus)
      Lede(text: String("A standard account on this Mac that only Atlas uses."))
        .padding(.top, Spacing.sPlus)
    }
    .frame(height: SampleWidth.column)
    .sampleSurface()
  }
#endif
