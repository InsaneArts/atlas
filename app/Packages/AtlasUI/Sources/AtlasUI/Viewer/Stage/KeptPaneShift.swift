import SwiftUI

/// How Activity, Apps and Screenshots show and hide as the tab changes once each has been shown: the pane is kept, so
/// a switch back fades the one already built instead of building it again, which took the main thread 15 to 45
/// milliseconds a switch. It fades as ``PaneShift`` fades a pane: in over the old one on ``PaneShift/arrival``, out on
/// ``PaneShift/departure``, turned back from where it is when the switch is reversed mid-way. Its scroll indicators
/// wait until it has faded in (``EnvironmentValues/paneSettled``), so none flashes over the crossfade. Hidden, it takes
/// no clicks, keyboard focus or VoiceOver, and tells its views it is not showing (``EnvironmentValues/paneShows``).
struct KeptPaneShift: ViewModifier {
  /// Whether the pane shows.
  let isShown: Bool
  /// Whether the pane draws, faded in or out on its own animation as ``isShown`` changes; it is first built showing.
  @State private var drawn = true
  /// Whether the pane has finished fading in, so its scroll indicators may show.
  @State private var settled = true

  /// The pane, faded while hidden.
  func body(content: Content) -> some View {
    content
      .environment(\.paneShows, isShown)
      .environment(\.paneSettled, settled)
      .opacity(drawn ? 1 : 0)
      .allowsHitTesting(isShown)
      .disabled(!isShown)
      .accessibilityHidden(!isShown)
      .zIndex(isShown ? 1 : 0)
      .onChange(of: isShown) { _, shown in fade(shown) }
  }

  /// Fades the pane in or out; once in, its scroll indicators may show.
  ///
  /// - Parameter shown: Whether the pane shows now.
  private func fade(_ shown: Bool) {
    settled = false
    withAnimation(shown ? PaneShift.arrival : PaneShift.departure) {
      drawn = shown
    } completion: {
      // A fade turned back before it ended leaves the pane hidden, and its indicators with it.
      settled = drawn
    }
  }
}
