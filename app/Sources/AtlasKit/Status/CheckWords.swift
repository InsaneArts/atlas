/// The helper's readiness checks in the app's words: each check the service names (`src/lib/session.rs`) gets a plain
/// name and, when it fails, one plain sentence with no command in it, since the viewer offers buttons, not commands.
package enum CheckWords {
  /// A check whose state cannot be observed yet.
  package static let notKnown = "Not known yet"
  /// A check that passed.
  package static let ready = "Ready"

  /// The check's name as the Ready card shows it.
  ///
  /// - Parameter name: The check's name, as the service gives it.
  /// - Returns: A short plain name; a check this app does not know keeps its own.
  package static func title(_ name: String) -> String {
    switch name {
    case "Helper running", "Helper reachable from here": "Helper"
    case "Helper in the background": "In the background"
    case "Screen Recording allowed": "Screen Recording"
    case "Accessibility allowed": "Accessibility"
    case "Input service ready": "Mouse and keyboard"
    default: name
    }
  }

  /// What a failed check means, in one plain sentence.
  ///
  /// - Parameter check: The failed check.
  /// - Returns: What is wrong and, when the app can do it, what to press.
  package static func failed(_ check: ViewerStatus.Check) -> String {
    let step = check.nextStep ?? ""
    // The service's own step, when it says to start input again, is what Recover does.
    let restartsInput = step.contains("start-input")
    switch check.name {
    case "Helper running": return "Atlas isn't answering in the helper."
    case "Helper reachable from here": return "This app can't reach the helper."
    case "Helper in the background" where step.hasPrefix("Switch back"):
      return "The helper is on screen. Switch back to your own account."
    case "Helper in the background": return "The helper isn't running in the background."
    case "Screen Recording allowed": return "Turn on Atlas in the helper's Screen Recording settings."
    case "Accessibility allowed" where restartsInput, "Input service ready" where restartsInput:
      return "Mouse and keyboard stopped. Recover starts them again."
    case "Accessibility allowed": return "Turn on Atlas in the helper's Accessibility settings."
    case "Input service ready" where step.contains("Accessibility"): return "Needs Accessibility first."
    case "Input service ready": return "Mouse and keyboard aren't ready."
    default: return step.isEmpty || step.contains("atlas ") ? "Not ready." : Words.sentence(step)
    }
  }
}
