extension OnboardingScreen {
  /// The welcome's headline.
  static let welcomeTitle = "Give your agents"
  /// Its second line.
  static let welcomeTail = "a Mac of their own."
  /// The account choice's headline.
  static let chooseTitle = "Agents need an account"
  /// Its second line.
  static let chooseTail = "of their own."
  /// The account form's headline.
  static let createTitle = "Create the helper account."
  /// The switch screen's second line.
  static let switchTail = "Just this once, to set Atlas up there."
  /// The agents screen's headline.
  static let agentsTitle = "Teach your agents"
  /// Its second line.
  static let agentsTail = "to test on screen."
  /// The done screen's headline.
  static let doneTitle = "You’re set."
  /// The allow screen's second line.
  static let allowTail = "through Atlas?"
  /// The permissions screen's headline.
  static let permissionsTitle = "Let Atlas see and use"
  /// Its second line.
  static let permissionsTail = "this screen."
  /// The helper's done screen's headline.
  static let helperDoneTitle = "All set here."
  /// The first reading's headline once it failed.
  static let loadingFailedTitle = "Atlas couldn’t start its setup."
  /// The blocked screen's headline.
  static let blockedTitle = "One thing first."

  /// The screen's headline as VoiceOver says it when the screen arrives: its statement and its second line.
  var spokenHeadline: String {
    let lines: [String] =
      switch self {
      case .welcome: [Self.welcomeTitle, Self.welcomeTail]
      case .choose: [Self.chooseTitle, Self.chooseTail]
      case .create(let state): [Self.createTitle, state.tail]
      case .switchToHelper(let state): [state.title, Self.switchTail]
      case .finishing(let state): [state.title, state.tail]
      case .agents: [Self.agentsTitle, Self.agentsTail]
      case .done: [Self.doneTitle]
      case .allow(let state): [state.title, Self.allowTail]
      case .permissions: [Self.permissionsTitle, Self.permissionsTail]
      case .helperDone(let state): [Self.helperDoneTitle, state.tail]
      case .closeLeftovers(let state): [state.title, state.lede ?? ""]
      case .blocked(let reason): [Self.blockedTitle, reason]
      }
    return lines.joined(separator: " ")
  }
}
