import AtlasKit

/// What Atlas says outside its windows: each notification's title and body, and the Dock badge.
enum NotificationWords {
  /// The Dock badge while Atlas needs the person: one thing waits on them.
  static let badge = "1"

  /// A notification's words.
  struct Note: Equatable {
    /// The bold first line.
    let title: String
    /// The sentence under it.
    let body: String
  }

  /// The words for what Atlas tells the person.
  ///
  /// - Parameter notice: What to tell them.
  /// - Returns: The notification's title and body.
  static func note(_ notice: RecoveryNotice) -> Note {
    switch notice {
    case .unfinished:
      Note(title: "Atlas needs you", body: "An action may not have finished in the helper. Open Atlas to recover.")
    case .inputStopped:
      Note(title: "Atlas needs you", body: "Mouse and keyboard stopped in the helper. Open Atlas to start them again.")
    case .loggedOut:
      Note(title: "The helper logged out", body: "Agents can't use it until you log in to it again.")
    }
  }
}
