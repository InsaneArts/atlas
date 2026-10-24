/// The screen's words while there is no picture to show: connecting, a logged-out helper, a helper that is not
/// ready, and a picture paused before the first arrived.
extension ViewerWords {
  /// Connecting's headline.
  static let connectingHeadline = "Connecting to the helper…"
  /// Connecting's sentence.
  static let connectingMessage = "This takes a moment after the Mac starts."
  /// Connecting again, after the picture was lost, over the last picture (v3 X4, R12).
  static let reconnectingHeadline = "Reconnecting…"
  /// A logged-out helper's headline.
  static let loggedOutHeadline = "The helper is logged out"
  /// A logged-out helper's sentence.
  static let loggedOutMessage = "Agents can't use it until you log in to it again."
  /// Not ready's headline.
  static let notReadyHeadline = "Atlas isn't answering in the helper"
  /// Not ready's sentence.
  static let notReadyMessage = "Recover checks the helper and starts it again."
  /// A picture paused before the first one arrived: the headline.
  static let pausedHeadline = "Picture paused"
  /// A picture paused before the first one arrived: the sentence.
  static let pausedMessage = "Work in the helper goes on."
}
