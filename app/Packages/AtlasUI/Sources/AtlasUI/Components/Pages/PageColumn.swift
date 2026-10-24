import SwiftUI

/// A page's column of one width, anchored to the top of its page, its pieces stacked with no gaps of their own: each
/// piece brings its own space above it, as its window's rhythm says. It never centres and never scrolls, so a piece
/// that appears or grows below, such as a status line, moves nothing above it, and the page is laid out once. A page
/// whose pieces outgrow their room is a mistake its window's tests catch, never something hidden under a fade.
struct PageColumn<Content: View>: View {
  /// The column's widest.
  private let width: CGFloat
  /// How far below the page's top the first piece starts.
  private let top: CGFloat
  /// The pieces, top to bottom.
  private let content: Content

  /// Creates a column.
  ///
  /// - Parameters:
  ///   - width: The column's widest: one width for every page of a window, so its edges never shift.
  ///   - top: How far below the page's top the first piece starts: one height for every page of a window, so the
  ///     pages' tops line up.
  ///   - content: The pieces, top to bottom.
  init(width: CGFloat, top: CGFloat, @ViewBuilder content: () -> Content) {
    self.width = width
    self.top = top
    self.content = content()
  }

  /// The pieces from the top, the room below them left empty.
  var body: some View {
    VStack(spacing: .zero) {
      content
    }
    .frame(maxWidth: width)
    .padding(.horizontal, Spacing.xxl)
    .padding(.top, top)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
  }
}

#if DEBUG
  #Preview("A title over a lede") {
    PageColumn(width: SampleWidth.column, top: Spacing.l) {
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
