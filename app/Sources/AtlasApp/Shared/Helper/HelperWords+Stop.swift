import AtlasService
import Foundation

/// How the person's Stop went, as every window that offers it says.
extension HelperWords {
  /// What a Stop came to: "Agents stopped." only when every background job the helper knows of was told to end;
  /// otherwise how many it could not end. A Stop that failed on a desktop names it, and the desktops it did stop,
  /// instead. What the service warned of follows either way, in its own words.
  ///
  /// - Parameter outcome: What the Stop came to.
  /// - Returns: "Agents stopped.", "Agents stopped, but 2 background jobs couldn't be stopped. …" or "Agents stopped
  ///   on main, but Atlas couldn't stop them on work."
  static func stopped(_ outcome: StopOutcome) -> String {
    let failed = outcome.failed.formatted(.list(type: .and))
    let stopped = outcome.stopped.formatted(.list(type: .and))
    let head =
      if !outcome.failed.isEmpty, outcome.stopped.isEmpty {
        "Atlas couldn't stop agents on \(failed)."
      } else if !outcome.failed.isEmpty {
        "Agents stopped on \(stopped), but Atlas couldn't stop them on \(failed)."
      } else if outcome.isComplete {
        "Agents stopped."
      } else if outcome.jobsLeft == 1 {
        "Agents stopped, but 1 background job couldn't be stopped. It may still run in the helper."
      } else {
        "Agents stopped, but \(outcome.jobsLeft) background jobs couldn't be stopped. They may still run in the helper."
      }
    let warned = outcome.warnings
      .map(Words.sentence)
      .filter { !$0.isEmpty }
    let sentences = [head] + warned
    return sentences.joined(separator: " ")
  }
}
