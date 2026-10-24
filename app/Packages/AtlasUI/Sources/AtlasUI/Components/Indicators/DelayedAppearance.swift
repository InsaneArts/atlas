import SwiftUI

/// Holds a view back for ``Motion/appear``'s delay and then fades it in, so a quick answer never flashes it (v3 R12,
/// S9): a spinner, or a placeholder. The wait is the animation's own delay, never a sleep. Only opacity changes, so
/// Reduce Motion keeps it; a render, which turns ``SwiftUICore/EnvironmentValues/animatesArrivals`` off, draws the
/// view at once.
struct DelayedAppearance: ViewModifier {
  @State private var isShown = false
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The view, hidden until its delay has passed.
  ///
  /// - Parameter content: The view to hold back.
  /// - Returns: The view, fading in after the delay.
  func body(content: Content) -> some View {
    content
      .opacity(isShown || !animatesArrivals ? 1 : 0)
      .onAppear {
        withAnimation(Motion.appear) { isShown = true }
      }
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var attempt = 0
    InteractivePreview {
      Button(String("Show again")) { attempt += 1 }
    } content: {
      Spinner()
        .appearsAfterDelay()
        .id(attempt)
    }
  }
#endif
