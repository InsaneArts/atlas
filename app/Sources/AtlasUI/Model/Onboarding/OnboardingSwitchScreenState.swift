/// The screen that sends the person to the helper once.
package struct OnboardingSwitchScreenState: Equatable, Sendable {
  /// Where the three steps stand: log in, Atlas opening, allow it.
  package struct Progress: Equatable, Sendable {
    /// Lock and switch, then log in.
    package var logIn: StepState
    /// Atlas opens in the helper by itself.
    package var openAtlas: StepState
    /// Allow Atlas twice, then switch back.
    package var allow: StepState

    /// Creates the three steps' progress.
    ///
    /// - Parameters:
    ///   - logIn: Lock and switch, then log in.
    ///   - openAtlas: Atlas opens in the helper by itself.
    ///   - allow: Allow Atlas twice, then switch back.
    package init(logIn: StepState, openAtlas: StepState, allow: StepState) {
      self.logIn = logIn
      self.openAtlas = openAtlas
      self.allow = allow
    }
  }

  /// The headline, naming the helper.
  package var title: String
  /// The first step's detail: where to choose the helper.
  package var logInDetail: String
  /// Where the three steps stand.
  package var progress: Progress
  /// Whether Lock and Switch shows: until the helper is signed in on screen.
  package var showsLockButton: Bool
  /// Whether Atlas opens in the helper by itself; false when the helper could not be readied, so the person opens it.
  package var opensByItself: Bool
  /// What happens next, in the pill; nil while the work note has something to say.
  package var waiting: String?

  /// Creates the switch screen's state.
  ///
  /// - Parameters:
  ///   - title: The headline, naming the helper.
  ///   - logInDetail: The first step's detail: where to choose the helper.
  ///   - progress: Where the three steps stand.
  ///   - showsLockButton: Whether Lock and Switch shows: until the helper is signed in on screen.
  ///   - opensByItself: Whether Atlas opens in the helper by itself.
  ///   - waiting: What happens next, in the pill; nil while the work note has something to say.
  package init(
    title: String,
    logInDetail: String,
    progress: Progress,
    showsLockButton: Bool,
    opensByItself: Bool = true,
    waiting: String? = nil
  ) {
    self.title = title
    self.logInDetail = logInDetail
    self.progress = progress
    self.showsLockButton = showsLockButton
    self.opensByItself = opensByItself
    self.waiting = waiting
  }
}
