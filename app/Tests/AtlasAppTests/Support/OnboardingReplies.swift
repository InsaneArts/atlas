/// `atlas onboarding status` replies the setup window's tests read, from either side.
enum OnboardingReplies {
  /// A fresh Mac: no helper chosen, and no standard account that could be one.
  static let freshMac = """
    {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
     "configured": false, "candidates": []}
    """

  /// A set-up Mac: the helper `atlas` is chosen, allowed and checked, so the agents come next.
  static let finishedMac = """
    {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
     "configured": true, "finished": true, "candidates": [],
     "helper": {"exists": true, "name": "atlas", "administrator": false, "paired": true, "signedIn": true,
                "answering": true, "hidden": true, "permissions": {"screenCapture": true, "accessibility": true}}}
    """

  /// A Mac whose helper is chosen, allowed and answering, but not checked yet: the end-to-end check runs next.
  static let checkingMac = finishedMac.replacingOccurrences(of: #""finished": true"#, with: #""finished": false"#)

  /// A Mac whose helper `atlas` is chosen and asked, but not logged in yet: the switch shows, with Lock and Switch.
  static let switchingMac = """
    {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
     "configured": true, "candidates": [],
     "helper": {"exists": true, "name": "atlas", "administrator": false, "paired": true, "signedIn": false,
                "answering": false}}
    """

  /// A helper-side reply while Atlas's job restarts: installed, not running, its permissions unknown.
  static let restarting = """
    {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
     "helper": {"controller": {"uid": 501, "name": "me"}, "installed": true, "running": false, "permissions": null}}
    """

  /// A helper-side reply for the account `me`, with Atlas running and its two permissions as given.
  ///
  /// - Parameters:
  ///   - asker: The account that asked, or nil when no one did.
  ///   - accessibility: Whether Accessibility is on.
  ///   - screenCapture: Whether Screen Recording is on.
  /// - Returns: The reply.
  static func helper(asker: String? = "me", accessibility: Bool, screenCapture: Bool = false) -> String {
    let controller = asker.map { #"{"uid": 501, "name": "\#($0)"}"# } ?? "null"
    return """
      {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
       "helper": {"controller": \(controller), "installed": true, "running": true,
                  "permissions": {"accessibility": \(accessibility), "screenCapture": \(screenCapture)}}}
      """
  }
}
