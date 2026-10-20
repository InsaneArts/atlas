/// Everything the viewer window draws, decided before it reaches the view.
package struct ViewerState: Equatable, Sendable {
  /// The title bar: the helper's state and the sidebar's toggle.
  package var header: ViewerHeaderState
  /// The helper's desktop and what is laid over it.
  package var stage: ViewerStageState
  /// The recovery alert over the desktop and footer; nil shows none.
  package var recovery: ViewerRecoveryPrompt?
  /// The line of what the person can do now, and the buttons beside it.
  package var footer: ViewerFooterState
  /// The cards beside the desktop, while the sidebar shows.
  package var sidebar: ViewerSidebarState

  /// Creates the viewer window's state.
  ///
  /// - Parameters:
  ///   - header: The title bar: the helper's state and the sidebar's toggle.
  ///   - stage: The helper's desktop and what is laid over it.
  ///   - recovery: The recovery alert over the desktop and footer; nil shows none.
  ///   - footer: The line of what the person can do now, and the buttons beside it.
  ///   - sidebar: The cards beside the desktop, while the sidebar shows.
  package init(
    header: ViewerHeaderState,
    stage: ViewerStageState,
    recovery: ViewerRecoveryPrompt? = nil,
    footer: ViewerFooterState,
    sidebar: ViewerSidebarState
  ) {
    self.header = header
    self.stage = stage
    self.recovery = recovery
    self.footer = footer
    self.sidebar = sidebar
  }
}
