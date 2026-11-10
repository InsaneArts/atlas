import AtlasCommands

/// The Privacy page's words: whether the home folder is private from the helper, and the alerts that ask
/// before opening it again or letting agents copy sign-ins in.
extension SettingsWords {
  /// The home folder is private.
  static let homePrivate = "Your home folder is private from the helper"
  /// What still works while it is.
  static let homePrivateDetail = "Agents still open your builds: Atlas copies them in first"
  /// The private row's button.
  static let undo = "Undo…"
  /// The open row's button.
  static let makePrivate = "Make Private"
  /// While a change to the person's files runs.
  static let changingFiles = "Changing…"
  /// Undo's alert.
  static let openHomeTitle = "Let the helper open your home folder again?"
  /// What Undo lets agents do.
  static let openHomeMessage = "Agents working in the helper could read the folders you made, such as your projects."
  /// Undo's button that goes ahead.
  static let letItOpen = "Let It Open"
  /// Undo's default: nothing changes.
  static let keepPrivate = "Keep Private"
  /// The sign-ins switch's alert.
  static let allowSignInsTitle = "Let agents copy your sign-ins into the helper?"
  /// What turning it on lets agents do.
  static let allowSignInsMessage =
    "An agent could sign the helper in to your accounts without you, by copying a browser's cookies."
  /// The sign-ins alert's button that goes ahead.
  static let allow = "Allow"
  /// The sign-ins alert's default: nothing changes.
  static let dontAllow = "Don't Allow"

  /// What the open row says: how many folders the helper can open, or that the home folder isn't private when it
  /// can open none now.
  ///
  /// - Parameter readable: How many folders the helper can open.
  /// - Returns: The row's title.
  static func homeOpen(_ readable: Int) -> String {
    switch readable {
    case 0: "Your home folder isn't private from the helper"
    case 1: "The helper can open 1 of your folders"
    default: "The helper can open \(readable) of your folders"
    }
  }

  /// The open row's line: the first folders the helper can open, or what Make Private does when there are none.
  ///
  /// - Parameter examples: The first folders, by name.
  /// - Returns: The line, such as "Such as dev and .config".
  static func homeOpenDetail(_ examples: [String]) -> String {
    switch examples.count {
    case 0: "Make it private so the folders you make stay yours"
    case 1: "Such as \(examples[0])"
    default: "Such as \(examples[0]) and \(examples[1])"
    }
  }

  /// The one-time offer's question.
  static let homeOfferTitle = "Keep your home folder private from the helper?"
  /// When making it private failed.
  static let homeNotMadePrivate = "Atlas couldn't make your home folder private"

  /// What the one-time offer says: what the helper can open now, and that builds still open.
  ///
  /// - Parameter files: The home folder as Atlas read it.
  /// - Returns: The alert's message.
  static func homeOfferMessage(_ files: HomeFiles) -> String {
    let examples = files.examples
    let such =
      switch (files.readable, examples.count) {
      case (0, _), (_, 0): ""
      case (_, 1): ", such as \(examples[0])"
      default: ", such as \(examples[0]) and \(examples[1])"
      }
    return "\(homeOpen(files.readable))\(such). Agents still open your builds: Atlas copies them in first."
  }
}
