/// Where the switch screen's three steps stand: log in to the helper, open Atlas there, allow it.
package struct SwitchProgress: Equatable, Sendable {
  /// Log in to the helper.
  package let logIn: StepProgress
  /// Open Atlas in the helper.
  package let openAtlas: StepProgress
  /// Allow Atlas there, twice.
  package let allow: StepProgress

  /// Creates the progress.
  ///
  /// - Parameters:
  ///   - logIn: Log in to the helper.
  ///   - openAtlas: Open Atlas in the helper.
  ///   - allow: Allow Atlas there.
  package init(logIn: StepProgress, openAtlas: StepProgress, allow: StepProgress) {
    self.logIn = logIn
    self.openAtlas = openAtlas
    self.allow = allow
  }
}
