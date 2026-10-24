/// Everything the setup window draws, decided before it reaches the view.
public struct OnboardingState: Equatable, Sendable {
  /// What fills the window: the first reading, the permission panel beside System Settings, or a screen.
  public var content: OnboardingContent
  /// The bar under it: Back, the track and the screen's main action.
  public var footer: OnboardingFooterState
  /// The line under a screen's controls.
  public var note: WorkNote
  /// Whether a command runs, so every button that starts one waits.
  public var isBusy: Bool
  /// Which way the window moved to this screen, so it slides in the way the person goes.
  public var direction: PageDirection

  /// Creates the setup window's state.
  ///
  /// - Parameters:
  ///   - content: What fills the window: the first reading, the permission panel beside System Settings, or a screen.
  ///   - footer: The bar under it: Back, the track and the screen's main action.
  ///   - note: The line under a screen's controls.
  ///   - isBusy: Whether a command runs, so every button that starts one waits.
  ///   - direction: Which way the window moved to this screen; forward unless it went back.
  public init(
    content: OnboardingContent,
    footer: OnboardingFooterState,
    note: WorkNote,
    isBusy: Bool,
    direction: PageDirection = .forward
  ) {
    self.content = content
    self.footer = footer
    self.note = note
    self.isBusy = isBusy
    self.direction = direction
  }
}
