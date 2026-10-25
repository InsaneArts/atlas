/// What the helper-command scenarios share.
enum HelperCommandsScenario {
  /// A status of a ready, idle helper the panel can switch to.
  static let idle: [String: Any] = [
    "ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle", "account": "atlas"],
  ]
}
