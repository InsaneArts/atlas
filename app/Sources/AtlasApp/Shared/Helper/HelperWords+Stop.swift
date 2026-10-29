import AtlasService

/// How the person's Stop went, as every window that offers it says.
extension HelperWords {
  /// What a Stop came to: "Agent stopped." only when every background job the helper knows of was told to end;
  /// otherwise how many it could not end. What the service warned of follows either way, in its own words.
  ///
  /// - Parameter outcome: What the Stop came to.
  /// - Returns: "Agent stopped." or "Agents stopped, but 2 background jobs couldn't be stopped. …"
  static func stopped(_ outcome: StopOutcome) -> String {
    let head =
      if outcome.isComplete {
        "Agent stopped."
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
