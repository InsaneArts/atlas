import SwiftUI

/// Fades one piece of a page in as the page arrives, 30 ms after the piece before it (``Motion/pieceArrival(_:)``),
/// so a new page's header, content and status come in one after another (v3 S2, X7). It plays once, as the piece
/// appears; later changes to the piece draw in place. Under Reduce Motion the piece arrives with its page's crossfade,
/// and a render, which turns ``SwiftUICore/EnvironmentValues/animatesArrivals`` off, draws it at once.
struct PageArrival: ViewModifier {
  /// Which piece of the page this is.
  let piece: PagePiece
  @State private var isShown = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The piece, faded in after the pieces before it.
  ///
  /// - Parameter content: The piece.
  /// - Returns: The piece, arriving in its place.
  func body(content: Content) -> some View {
    content
      .opacity(isShown || !animatesArrivals || reduceMotion ? 1 : 0)
      .onAppear {
        withAnimation(Motion.pieceArrival(piece.rawValue)) { isShown = true }
      }
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var arrival = 0
    InteractivePreview {
      Button(String("Arrive again")) { arrival += 1 }
    } content: {
      VStack(spacing: Spacing.m) {
        Text(String("Header"))
          .pagePiece(.header)
        Text(String("Content"))
          .pagePiece(.content)
        Text(String("Status"))
          .pagePiece(.status)
      }
      .foregroundStyle(Color.textPrimary)
      .id(arrival)
    }
  }
#endif
