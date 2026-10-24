import SwiftUI

/// Holds a view back for a delay and then fades it in, so a quick answer never flashes it: a spinner or a placeholder
/// after ``Motion/appear``'s delay, a working note after ``Motion/noteAppear``'s shorter one. The wait is the
/// animation's own delay, never a sleep. Only opacity changes, so Reduce Motion keeps it; a render, which turns
/// ``SwiftUICore/EnvironmentValues/animatesArrivals`` off, draws the view at once. A view that does not hold, such as a
/// problem line, shows at once, and one that shows does not hide again when it starts to hold.
struct DelayedAppearance: ViewModifier {
  /// The fade, after its delay.
  var appearing = Motion.appear
  /// Whether the view waits as it appears; false shows it at once.
  var holds = true
  @State private var isShown = false
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The view, hidden until its delay has passed.
  ///
  /// - Parameter content: The view to hold back.
  /// - Returns: The view, fading in after the delay.
  func body(content: Content) -> some View {
    content
      .opacity(isShown || !holds || !animatesArrivals ? 1 : 0)
      .onAppear {
        if holds {
          withAnimation(appearing) { isShown = true }
        } else {
          isShown = true
        }
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
