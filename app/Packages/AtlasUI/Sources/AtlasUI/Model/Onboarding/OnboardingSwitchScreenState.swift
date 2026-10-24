/// The screen that sends the person to the helper once.
public struct OnboardingSwitchScreenState: Equatable, Sendable {
  /// Where the three steps stand: log in, Atlas opening, allow it.
  public struct Progress: Equatable, Sendable {
    /// Lock and switch, then log in.
    public var logIn: StepState
    /// Atlas opens in the helper by itself.
    public var openAtlas: StepState
    /// Allow Atlas twice, then switch back.
    public var allow: StepState

    /// Creates the three steps' progress.
    ///
    /// - Parameters:
    ///   - logIn: Lock and switch, then log in.
    ///   - openAtlas: Atlas opens in the helper by itself.
    ///   - allow: Allow Atlas twice, then switch back.
    public init(logIn: StepState, openAtlas: StepState, allow: StepState) {
      self.logIn = logIn
      self.openAtlas = openAtlas
      self.allow = allow
    }
  }

  /// The headline, naming the helper.
  public var title: String
  /// The first step's detail: where to choose the helper.
  public var logInDetail: String
  /// Where the three steps stand.
  public var progress: Progress
  /// Whether Atlas opens in the helper by itself; false when the helper could not be readied, so the person opens it.
  public var opensByItself: Bool
  /// What happens next, in the pill; nil while the work note has something to say.
  public var waiting: String?

  /// Creates the switch screen's state.
  ///
  /// - Parameters:
  ///   - title: The headline, naming the helper.
  ///   - logInDetail: The first step's detail: where to choose the helper.
  ///   - progress: Where the three steps stand.
  ///   - opensByItself: Whether Atlas opens in the helper by itself.
  ///   - waiting: What happens next, in the pill; nil while the work note has something to say.
  public init(
    title: String,
    logInDetail: String,
    progress: Progress,
    opensByItself: Bool = true,
    waiting: String? = nil
  ) {
    self.title = title
    self.logInDetail = logInDetail
    self.progress = progress
    self.opensByItself = opensByItself
    self.waiting = waiting
  }
}
