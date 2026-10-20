/// Reads the app's arguments into what this launch does. The order is the one the app has always used: the flag
/// modes first, each before the next, then the app that reads its viewer URL on standard input.
enum LaunchArguments {
  /// What `arguments` ask this launch to do.
  ///
  /// - Parameter arguments: The process's arguments, its own path first.
  /// - Returns: The launch mode.
  static func mode(_ arguments: [String]) -> LaunchMode {
    if let title = value(after: "--notice", in: arguments), let message = value(after: "--notice", in: arguments, at: 2)
    {
      return .notice(title: title, message: message, switchBack: value(after: "--switch-back", in: arguments))
    }
    if let change = value(after: "--login", in: arguments) {
      return .login(change)
    }
    if arguments.contains("--onboarding") {
      return .onboarding
    }
    if let folder = value(after: "--render-onboarding", in: arguments) {
      return .renderOnboarding(folder)
    }
    if let folder = value(after: "--render-viewer", in: arguments) {
      return .renderViewer(folder)
    }
    return .app(render: value(after: "--render", in: arguments), dark: arguments.contains("--dark"))
  }

  /// The argument `offset` places after `flag`, if the flag is there and the argument is.
  ///
  /// - Parameters:
  ///   - flag: The flag.
  ///   - arguments: The process's arguments.
  ///   - offset: How far after the flag, 1 for the next one.
  /// - Returns: The argument, or nil.
  private static func value(after flag: String, in arguments: [String], at offset: Int = 1) -> String? {
    guard let index = arguments.firstIndex(of: flag), index + offset < arguments.count else { return nil }
    return arguments[index + offset]
  }
}
