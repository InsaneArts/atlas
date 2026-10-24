/// Everything the setup window draws, decided before it reaches the view.
public struct OnboardingState: Equatable, Sendable {
  /// What fills the window: the first reading, the permission panel beside System Settings, or a screen.
  public var content: OnboardingContent
  /// The bar under a screen: Back, the next step's button and the dots.
  public var footer: OnboardingFooterState
  /// The line under a screen's controls.
  public var note: WorkNote
  /// Whether a command runs, so every button that starts one waits.
  public var isBusy: Bool

  /// Creates the setup window's state.
  ///
  /// - Parameters:
  ///   - content: What fills the window: the first reading, the permission panel beside System Settings, or a screen.
  ///   - footer: The bar under a screen: Back, the next step's button and the dots.
  ///   - note: The line under a screen's controls.
  ///   - isBusy: Whether a command runs, so every button that starts one waits.
  public init(content: OnboardingContent, footer: OnboardingFooterState, note: WorkNote, isBusy: Bool) {
    self.content = content
    self.footer = footer
    self.note = note
    self.isBusy = isBusy
  }
}
