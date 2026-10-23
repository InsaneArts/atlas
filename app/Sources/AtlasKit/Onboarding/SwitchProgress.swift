/// Where the switch screen's three steps stand: log in to the helper, Atlas opening there, allow it.
package struct SwitchProgress: Equatable, Sendable {
  /// Log in to the helper.
  package let logIn: StepProgress
  /// Atlas opens in the helper by itself.
  package let openAtlas: StepProgress
  /// Allow Atlas there, twice.
  package let allow: StepProgress

  /// Creates the progress.
  ///
  /// - Parameters:
  ///   - logIn: Log in to the helper.
  ///   - openAtlas: Atlas opens in the helper by itself.
  ///   - allow: Allow Atlas there.
  package init(logIn: StepProgress, openAtlas: StepProgress, allow: StepProgress) {
    self.logIn = logIn
    self.openAtlas = openAtlas
    self.allow = allow
  }
}
