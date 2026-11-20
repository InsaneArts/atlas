import AtlasService
import Foundation

/// How the person's Stop went, as every window that offers it says.
extension HelperWords {
  /// What a Stop came to: "Agents stopped." only when every background job the helper knows of was told to end;
  /// otherwise how many it could not end. A Stop that failed on a desktop, or that a desktop has not confirmed, names
  /// it, and the desktops it did stop, instead, each by its title. What the service warned of follows either way, in
  /// its own words.
  ///
  /// - Parameters:
  ///   - outcome: What the Stop came to.
  ///   - titles: Each desktop's title by its name in the settings; a name with none is said as it is.
  ///   - noun: How the desktop shown is named, where jobs left may still run; the helper by default.
  /// - Returns: "Agents stopped.", "Agents stopped, but 2 background jobs couldn't be stopped. …", "Agents stopped on
  ///   Second Login, but Atlas couldn't stop them on work." or "Agents stopped on Second Login, but Atlas isn't sure
  ///   they stopped on work."
  static func stopped(_ outcome: StopOutcome, titles: [String: String] = [:], noun: DesktopNoun = .helper) -> String {
    let named = { (names: [String]) in
      names.map { titles[$0] ?? $0 }
        .formatted(.list(type: .and))
    }
    let unconfirmed = outcome.uncertain.filter { !outcome.failed.contains($0) }
    let failed = named(outcome.failed)
    let stopped = named(outcome.stopped)
    let unsure = named(unconfirmed)
    var sentences: [String] = []
    if !outcome.failed.isEmpty, outcome.stopped.isEmpty {
      sentences.append("Atlas couldn't stop agents on \(failed).")
    } else if !outcome.failed.isEmpty {
      sentences.append("Agents stopped on \(stopped), but Atlas couldn't stop them on \(failed).")
    } else if !unconfirmed.isEmpty, outcome.stopped.isEmpty {
      sentences.append("Atlas isn't sure agents stopped on \(unsure).")
    } else if !unconfirmed.isEmpty {
      sentences.append("Agents stopped on \(stopped), but Atlas isn't sure they stopped on \(unsure).")
    } else if outcome.isComplete {
      sentences.append("Agents stopped.")
    } else if outcome.jobsLeft == 1 {
      sentences.append("Agents stopped, but 1 background job couldn't be stopped. It may still run\(noun.jobsPlace).")
    } else {
      sentences.append(
        "Agents stopped, but \(outcome.jobsLeft) background jobs couldn't be stopped. They may still run\(noun.jobsPlace)."
      )
    }
    if !outcome.failed.isEmpty, !unconfirmed.isEmpty {
      sentences.append("Atlas isn't sure they stopped on \(unsure).")
    }
    let warned = outcome.warnings
      .map(Words.sentence)
      .filter { !$0.isEmpty }
    return (sentences + warned)
      .joined(separator: " ")
  }

  /// Whether a Stop's outcome has more to say than "Agents stopped.": a desktop that failed or is not sure, a job it
  /// could not end, or a warning.
  ///
  /// - Parameter outcome: What the Stop came to.
  /// - Returns: Whether the panel says it in place of its line.
  static func stopSaysMore(_ outcome: StopOutcome) -> Bool {
    !outcome.uncertain.isEmpty || !outcome.isComplete || !outcome.warnings.isEmpty
  }
}
