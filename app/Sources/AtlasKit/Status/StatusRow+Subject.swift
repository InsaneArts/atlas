extension StatusRow {
  /// What a row checks.
  package enum Subject: Hashable, Sendable {
    /// One of the service's readiness checks, by the name the service gives it.
    case check(String)
    /// Whether the helper's native Save and Open dialogs worked.
    case nativeDialogs
  }
}
