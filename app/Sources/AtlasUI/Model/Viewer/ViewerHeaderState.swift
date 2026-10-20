/// The viewer's title bar: the helper's state in its middle, the sidebar's toggle at its end.
package struct ViewerHeaderState: Equatable, Sendable {
  /// The helper's state as this window sees it, such as "You have control" or "Paused".
  package var status: StatusSummary
  /// Whether the status dot breathes: an agent is working in the helper.
  package var isWorking: Bool
  /// The helper and how fresh its picture is, after the status, such as "Atlas · Live", as of the state; the app's
  /// window reads it again as the status line draws (``ViewerView``), so the picture's age ticks there alone.
  package var detail: String
  /// The sidebar's toggle: on while the person asks for the sidebar, which shows once the window is wide enough.
  package var sidebar: ViewerToggle
  /// Whether the window is in full screen, which hides the window buttons the toggle otherwise keeps clear of.
  package var isFullScreen: Bool

  /// Creates the title bar's state.
  ///
  /// - Parameters:
  ///   - status: The helper's state as this window sees it, such as "You have control" or "Paused".
  ///   - isWorking: Whether the status dot breathes: an agent is working in the helper.
  ///   - detail: The helper and how fresh its picture is, after the status, such as "Atlas · Live".
  ///   - sidebar: The sidebar's toggle: on while the person asks for the sidebar, which shows once the window is wide
  ///     enough.
  ///   - isFullScreen: Whether the window is in full screen, which hides the window buttons the toggle otherwise
  ///     keeps clear of.
  package init(status: StatusSummary, isWorking: Bool, detail: String, sidebar: ViewerToggle, isFullScreen: Bool) {
    self.status = status
    self.isWorking = isWorking
    self.detail = detail
    self.sidebar = sidebar
    self.isFullScreen = isFullScreen
  }
}
