extension OnboardingFacts.Helper {
  /// The three steps on the switch screen: log in, Atlas opening, allow it.
  public var switchProgress: SwitchProgress {
    let allowed = accessibility && screenCapture
    if !signedIn { return SwitchProgress(logIn: .current, openAtlas: .waiting, allow: .waiting) }
    if !answering { return SwitchProgress(logIn: .done, openAtlas: .current, allow: .waiting) }
    if !allowed { return SwitchProgress(logIn: .done, openAtlas: .done, allow: .current) }
    return SwitchProgress(logIn: .done, openAtlas: .done, allow: .done)
  }
}
