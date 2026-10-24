import SwiftUI

/// A form's sections in a column at their insets (U-17), scrolling only when the window is shorter than they are,
/// and saying how tall they are, insets included, so a window can fit them (v3 G2).
struct FormColumn<Content: View>: View {
  /// The column's insets.
  private let insets: EdgeInsets
  /// Receives the column's height each time it changes, the first included.
  private let fits: @MainActor (CGFloat) -> Void
  /// The sections, or any content.
  private let content: Content

  /// Creates a column.
  ///
  /// - Parameters:
  ///   - insets: The column's insets.
  ///   - fits: Receives the column's height each time it changes, the first included; nothing by default.
  ///   - content: The sections, or any content.
  init(
    insets: EdgeInsets,
    fits: @escaping @MainActor (CGFloat) -> Void = { _ in },
    @ViewBuilder content: () -> Content
  ) {
    self.insets = insets
    self.fits = fits
    self.content = content()
  }

  /// The content at its insets, in a scroll view that bounces only when there is more to see.
  var body: some View {
    ScrollView {
      content
        .padding(insets)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .onGeometryChange(for: CGFloat.self) { proxy in
          proxy.size.height
        } action: { height in
          fits(height)
        }
    }
    .scrollIndicators(.automatic)
    .scrollBounceBehavior(.basedOnSize)
  }
}
