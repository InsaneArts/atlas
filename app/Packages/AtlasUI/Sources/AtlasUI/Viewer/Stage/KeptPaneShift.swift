import SwiftUI

/// How Activity, Apps and Screenshots show and hide as the tab changes once each has been shown: the pane is kept, so
/// a switch back fades the one already built instead of building it again, which took the main thread 15 to 45
/// milliseconds a switch. The shared ``PaneHandoff`` waits for the outgoing pane to disappear before admitting
/// the latest destination. Each pane fades on ``PaneShift/arrival`` and ``PaneShift/departure``.
/// Its scroll indicators wait until it has faded in (``EnvironmentValues/paneSettled``). Hidden, it takes
/// no clicks, keyboard focus or VoiceOver, and tells its views it is not showing (``EnvironmentValues/paneShows``).
struct KeptPaneShift: ViewModifier {
  /// Whether the pane shows.
  let isShown: Bool
  /// Whether the switch that shows or hides it is a change of depth, from or to the Screen (``ListRise``).
  let rises: Bool
  /// What the stage does once the pane has faded out, such as forget the pictures it drew.
  var hidden: @MainActor () -> Void = {}
  /// Whether the pane draws, faded in or out on its own animation as ``isShown`` changes; it starts clear on its first arrival.
  @State private var drawn = false
  /// Whether the pane has finished fading in, so its scroll indicators may show.
  @State private var settled = false
  /// Whether the pane sits below its place, as it does while hidden, so a rise starts from there.
  @State private var lowered = true

  /// Each switch invalidates completions from an interrupted fade.
  @State private var generation = 0
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The pane, faded while hidden.
  func body(content: Content) -> some View {
    content
      .environment(\.paneShows, isShown)
      .environment(\.paneSettled, settled)
      // A view drawn once, as a render is, draws the pane shown at once rather than at the start of its fade.
      .opacity(drawn || (isShown && !animatesArrivals) ? 1 : 0)
      .offset(y: ListRise.offset(isLowered: lowered && animatesArrivals))
      .allowsHitTesting(isShown)
      .disabled(!isShown)
      .accessibilityHidden(!isShown)
      .zIndex(isShown ? 1 : 0)
      .onChange(of: isShown, initial: true) { _, shown in fade(shown) }
  }

  /// Fades the pane in or out, rising in or sinking out on a change of depth; once in, its scroll indicators may show.
  /// Once hidden, it waits lowered, so the next rise starts below its place, and the stage is told.
  ///
  /// - Parameter shown: Whether the pane shows now.
  private func fade(_ shown: Bool) {
    generation += 1
    let current = generation
    settled = false
    if rises {
      withAnimation(ListRise.motion) { lowered = !shown }
    } else if shown {
      place(lowered: false)
    }
    withAnimation(shown ? PaneShift.arrival : PaneShift.departure, completionCriteria: .removed) {
      drawn = shown
    } completion: {
      // A fade turned back before it ended leaves the pane hidden, and its indicators with it.
      guard generation == current else { return }
      settled = drawn
      guard !drawn else { return }
      place(lowered: true)
      hidden()
    }
  }

  /// Moves the pane above or below its place at once, as it shows from another list or waits hidden.
  ///
  /// - Parameter isLowered: Whether it sits below its place.
  private func place(lowered isLowered: Bool) {
    var transaction = Transaction()
    transaction.disablesAnimations = true
    withTransaction(transaction) { lowered = isLowered }
  }
}
