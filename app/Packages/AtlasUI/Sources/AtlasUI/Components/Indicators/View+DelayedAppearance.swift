import SwiftUI

extension View {
  /// Shows the view only after ``Motion/appear``'s delay, fading it in, so a quick answer never flashes it.
  ///
  /// - Returns: The view, held back for its delay.
  func appearsAfterDelay() -> some View {
    modifier(DelayedAppearance())
  }

  /// Shows a working note only after ``Motion/noteAppear``'s delay, fading it in, so work that ends sooner never
  /// flashes it; a note that does not hold, such as a problem, shows at once.
  ///
  /// - Parameter holds: Whether the note waits as it appears: true while it says work runs.
  /// - Returns: The view, held back for its delay where it holds.
  func appearsAsNote(holds: Bool) -> some View {
    modifier(DelayedAppearance(appearing: Motion.noteAppear, holds: holds))
  }
}
