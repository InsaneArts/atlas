import Foundation

/// The notices over the screen: what Resume and a change in the helper did, why control ended, and text too long to
/// send. How Stop went is said as every window says it (``HelperWords/stopped(_:)``).
extension ViewerWords {
  /// Once Resume ran.
  static let agentResumed = "Agent resumed."
  /// Take Control still refused once the wait for the agent's step ran out: an agent's action holds the helper.
  static let agentMidAction = "The agent didn't finish its step. Try again."
  /// Once the helper restarted.
  static let helperRestarted = "The helper was restarted. Take control again when ready."
  /// The helper's session or display changed, which ended control.
  static let helperChanged = "The helper changed. Take control again when ready."
  /// The status said input is not ready, which ended control, when it gives no reason.
  static let notReady = "The helper isn't ready for mouse and keyboard."
  /// The service said no viewer holds control any more: it was released from the menu bar, say.
  static let releasedElsewhere = "Control ended outside this window. Take control again when ready."
  /// Take Control pressed while paused resumed the picture, and no fresh one came in time to aim at.
  static let noFreshPicture = "No fresh picture arrived. Take control again once one shows."
  /// A stale picture ended control.
  static let stalePicture = "The picture stopped updating. Wait for a fresh one, then take control again."
  /// Typed text held a grapheme longer than the helper takes in one go.
  static let sequenceTooLong = "This text contains a character sequence too long to type."

  /// A paste longer than the helper takes at once.
  ///
  /// - Parameter limit: The most Unicode characters one paste may carry.
  /// - Returns: "Paste is limited to 2,048 characters at a time; an emoji or an accent can count as more than one."
  static func pasteTooLong(_ limit: Int) -> String {
    "Paste is limited to \(limit.formatted()) characters at a time; an emoji or an accent can count as more than one."
  }

  /// Control ended because the status said input is not ready, in the status's own words.
  ///
  /// - Parameter reason: The status's reason; nil before it gives one, or empty, for none.
  /// - Returns: "Control ended. Mouse and keyboard stopped in the helper. Recover starts them again."
  static func controlEnded(_ reason: String?) -> String {
    let known = reason.flatMap { $0.isEmpty ? nil : $0 } ?? notReady
    return "Control ended. \(Words.sentence(known))"
  }
}
