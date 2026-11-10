import AtlasService
import AtlasStatus
import AtlasUI

/// Waiting for you: while a question waits for the person, the panel's heading says so in the attention colour, in
/// place of Idle or Working, the question's card at the top of its content.
extension PanelPresenter {
  /// Whether the heading says Waiting for you: a question is up while the helper is idle or an agent works there.
  /// Control, a stop, recovery and the rest keep their own word, as each says more.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  /// - Returns: Whether a question waits.
  static func waitsForYou(_ snapshot: PanelSnapshot, mode: Mode) -> Bool {
    (mode.kind == .idle || mode.kind == .working)
      && StatusReading(snapshot.status)
        .waitsForYou(from: snapshot.asks, at: snapshot.date)
  }

  /// The heading while a question waits: Waiting for you in the attention colour, with no line, as the card under it
  /// says the rest.
  static let waitingSummary = StatusSummary(label: AskWords.waiting, tone: .attention, mark: .dot)
}
