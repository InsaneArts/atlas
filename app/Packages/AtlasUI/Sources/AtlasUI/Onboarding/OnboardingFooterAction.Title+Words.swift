extension OnboardingFooterAction.Title {
  /// The button's title: fixed, or naming the permission it allows.
  var words: String {
    switch self {
    case .back: "Back"
    case .chooseAnother: "Choose Another Account"
    case .getStarted: "Get Started"
    case .continue: "Continue"
    case .createAccount: "Create Account"
    case .lockAndSwitch: "Lock and Switch"
    case .tryAgain: "Try Again"
    case .checkAgain: "Check Again"
    case .closeIt: "Close It…"
    case .done: "Done"
    case .allow: "Allow"
    case .notNow: "Not Now"
    case .allowPermission(let permission): "Allow \(permission)…"
    case .showInFinder: "Show in Finder"
    case .switchBack: "Switch Back"
    }
  }
}
