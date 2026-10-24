import AtlasService

/// The helper's readiness checks in the app's words: each check the service names (`src/lib/session.rs`) gets a plain
/// name and, when it fails, one plain sentence with no command in it, since the viewer offers buttons, not commands.
enum CheckWords {
  /// A check whose state cannot be observed yet.
  static let notKnown = "Not known yet"
  /// A check that passed.
  static let ready = "Ready"
  /// The native Save and Open check, as the Ready card names it.
  static let nativeDialogs = "Save and Open dialogs"
  /// The native Save and Open check before it ran in this session.
  static let notCheckedYet = "Not checked yet"

  /// The native Save and Open check passed.
  ///
  /// - Parameter ageSeconds: How long ago it ran, in seconds; nil when unknown.
  /// - Returns: "Worked 5 min ago", or "Worked" when its age is unknown.
  static func worked(ageSeconds: Double?) -> String {
    "Worked\(ago(ageSeconds))"
  }

  /// The native Save and Open check failed.
  ///
  /// - Parameter ageSeconds: How long ago it ran, in seconds; nil when unknown.
  /// - Returns: "Didn't work 5 min ago.", or "Didn't work." when its age is unknown.
  static func didNotWork(ageSeconds: Double?) -> String {
    "Didn't work\(ago(ageSeconds))."
  }

  /// The check's name as the Ready card shows it.
  ///
  /// - Parameter name: The check's name, as the service gives it.
  /// - Returns: A short plain name; a check this app does not know keeps its own.
  static func title(_ name: String) -> String {
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
  static func failed(_ check: ViewerStatus.Check) -> String {
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
      return "Mouse and keyboard stopped in the helper. Recover starts them again."
    case "Accessibility allowed": return "Turn on Atlas in the helper's Accessibility settings."
    case "Input service ready" where step.contains("Accessibility"): return "Needs Accessibility first."
    case "Input service ready": return "Mouse and keyboard aren't ready."
    default: return step.isEmpty || step.contains("atlas ") ? "Not ready." : Words.sentence(step)
    }
  }

  /// " 5 min ago" after a native check's word, or nothing when its age is unknown.
  private static func ago(_ seconds: Double?) -> String {
    seconds.map { " \(Words.ago($0))" } ?? ""
  }
}
