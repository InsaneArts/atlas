extension OnboardingFooterAction.Title {
  /// The button's fixed title.
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
    case .done: "Done"
    case .allow: "Allow"
    case .switchBack: "Switch Back"
    }
  }
}
