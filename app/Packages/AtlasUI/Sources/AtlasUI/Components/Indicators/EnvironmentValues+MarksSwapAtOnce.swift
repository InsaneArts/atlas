import SwiftUI

extension EnvironmentValues {
  /// Whether a check's mark swaps at once, for work that is held for ``Motion/Duration/leastWork`` and so always lasts
  /// long enough to show: its spinner shows with no wait and fades in with no scale, and the result that replaces it
  /// pops in. Off by default, where a spinner waits ``Motion/Duration/appearDelay`` so a quick answer never flashes
  /// one.
  @Entry var marksSwapAtOnce = false
}
