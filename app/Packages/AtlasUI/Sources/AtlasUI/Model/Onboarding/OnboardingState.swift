/// Everything the setup window draws, decided before it reaches the view.
public struct OnboardingState: Equatable, Sendable {
  /// What fills the window: the first reading, the permission panel beside System Settings, or a screen.
  public var content: OnboardingContent
  /// The bar under it: Back, the track and the screen's main action.
  public var footer: OnboardingFooterState
  /// The line under a screen's controls.
  public var note: WorkNote

  /// Creates the setup window's state.
  ///
  /// - Parameters:
  ///   - content: What fills the window: the first reading, the permission panel beside System Settings, or a screen.
  ///   - footer: The bar under it: Back, the track and the screen's main action.
  ///   - note: The line under a screen's controls.
  public init(content: OnboardingContent, footer: OnboardingFooterState, note: WorkNote) {
    self.content = content
    self.footer = footer
    self.note = note
  }
}
