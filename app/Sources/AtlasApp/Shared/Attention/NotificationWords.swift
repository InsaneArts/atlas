import AtlasStatus

/// What Atlas says outside its windows: each notification's title, body and actions.
/// `nonisolated`: constants and a pure function, which Notification Center's delegate reads off the main actor.
nonisolated enum NotificationWords {
  /// Whether The helper needs you offers Recover, its one action. The Recover action runs the alert's own recovery,
  /// with its guards, without asking first; set it to false to offer Open Atlas instead, where the alert asks. A click
  /// on the notification itself opens Atlas either way.
  static let offersRecover = true
  /// The notification's action that recovers the helper without opening Atlas.
  static let recover = "Recover"
  /// The notification's action that opens Atlas on its main window.
  static let openAtlas = "Open Atlas"
  /// The helper needs you's kind, which carries its action.
  static let attentionCategory = "atlas.attention"
  /// Recover's identifier, as Notification Center hands it back.
  static let recoverAction = "atlas.recover"
  /// Open Atlas's identifier, as Notification Center hands it back.
  static let openAction = "atlas.open"
  /// The helper needs you's title, for every reason.
  static let needsYou = "The helper needs you"

  /// A notification's words.
  struct Note: Equatable {
    /// The bold first line.
    let title: String
    /// The sentence under it.
    let body: String
    /// Whether it carries The helper needs you's action.
    let acts: Bool
    /// Whether it plays a sound: not for news the person needn't act on, such as the helper logging out.
    var sounds = true
  }

  /// The words for what Atlas tells the person: The helper needs you says the reason, and with Recover offered,
  /// nothing more, since its action says what to do next. The helper logging out is told quietly, with no sound.
  ///
  /// - Parameter notice: What to tell them.
  /// - Returns: The notification's title and body.
  static func note(_ notice: RecoveryNotice) -> Note {
    switch notice {
    case .unfinished:
      Note(
        title: needsYou,
        body: "An action may not have finished in the helper." + (offersRecover ? "" : " Open Atlas to recover."),
        acts: true
      )
    case .inputStopped:
      Note(
        title: needsYou,
        body: "Mouse and keyboard stopped in the helper." + (offersRecover ? "" : " Open Atlas to start them again."),
        acts: true
      )
    case .loggedOut:
      Note(
        title: "The helper logged out",
        body: "Agents can't use it until you log in to it again.",
        acts: false,
        sounds: false
      )
    }
  }

  /// The words when a Recover started from the notification did not run: The helper needs you again, saying why.
  ///
  /// - Parameter reason: Why, in plain words.
  /// - Returns: The notification's title and body.
  static func refusal(_ reason: String) -> Note {
    Note(title: needsYou, body: reason, acts: true)
  }
}
