/// The screen that sends the person to the helper once.
package struct OnboardingSwitchScreenState: Equatable, Sendable {
  /// Where the three steps stand: log in, open Atlas, allow it.
  package struct Progress: Equatable, Sendable {
    /// Lock and switch, then log in.
    package var logIn: StepState
    /// Open Atlas in the helper.
    package var openAtlas: StepState
    /// Allow Atlas twice, then switch back.
    package var allow: StepState

    /// Creates the three steps' progress.
    ///
    /// - Parameters:
    ///   - logIn: Lock and switch, then log in.
    ///   - openAtlas: Open Atlas in the helper.
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
  /// The second step's title: the welcome questions first, until the helper has logged in.
  package var openAtlasTitle: String
  /// Where the three steps stand.
  package var progress: Progress
  /// Whether Lock and Switch shows: until the helper is signed in on screen.
  package var showsLockButton: Bool
  /// What happens next, in the pill; nil while the work note has something to say.
  package var waiting: String?

  /// Creates the switch screen's state.
  ///
  /// - Parameters:
  ///   - title: The headline, naming the helper.
  ///   - logInDetail: The first step's detail: where to choose the helper.
  ///   - openAtlasTitle: The second step's title: the welcome questions first, until the helper has logged in.
  ///   - progress: Where the three steps stand.
  ///   - showsLockButton: Whether Lock and Switch shows: until the helper is signed in on screen.
  ///   - waiting: What happens next, in the pill; nil while the work note has something to say.
  package init(
    title: String,
    logInDetail: String,
    openAtlasTitle: String,
    progress: Progress,
    showsLockButton: Bool,
    waiting: String? = nil
  ) {
    self.title = title
    self.logInDetail = logInDetail
    self.openAtlasTitle = openAtlasTitle
    self.progress = progress
    self.showsLockButton = showsLockButton
    self.waiting = waiting
  }
}
