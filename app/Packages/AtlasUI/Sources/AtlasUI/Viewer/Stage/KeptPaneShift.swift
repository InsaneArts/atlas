import SwiftUI

/// How Activity, Apps and Screenshots show and hide as the tab changes once each has been shown (v3 X3): the pane is
/// kept, so a switch back moves and fades the one already built instead of building it again, which took the main
/// thread 15 to 45 milliseconds a switch and dropped the first frames of every slide. It moves as ``PaneShift`` moves
/// a pane: in from ``ViewerLayout/paneEnter`` on the side its tab lies, on ``Motion/tab`` after the overlap; out
/// ``ViewerLayout/paneLeave`` toward the other side on ``Motion/crossfadeOut``; turned back from where it is when the
/// switch is reversed mid-way. Hidden, it waits faded on the side the next switch would bring it from, takes no clicks,
/// keyboard focus or VoiceOver, and tells its views it is not showing (``EnvironmentValues/paneShows``). Under Reduce
/// Motion it only crossfades, over 0.15 seconds.
struct KeptPaneShift: ViewModifier {
  /// Whether the pane shows.
  let isShown: Bool
  /// Where it sits while hidden, from ``ViewerLayout/keptPaneOffset(_:shown:left:reduceMotion:)``.
  let offset: CGFloat
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The pane, faded and moved aside while hidden.
  func body(content: Content) -> some View {
    content
      .environment(\.paneShows, isShown)
      .opacity(isShown ? 1 : 0)
      .offset(x: isShown ? .zero : offset)
      .animation(animation, value: isShown)
      .allowsHitTesting(isShown)
      .disabled(!isShown)
      .accessibilityHidden(!isShown)
      .zIndex(isShown ? 1 : 0)
  }

  /// The change's animation: the arrival as it shows, the quick fade as it hides.
  private var animation: Animation {
    isShown
      ? PaneShift.arrival(reduceMotion: reduceMotion)
      : PaneShift.departure(reduceMotion: reduceMotion)
  }
}
