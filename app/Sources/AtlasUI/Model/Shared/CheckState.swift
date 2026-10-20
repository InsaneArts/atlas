/// How one line of a checklist stands.
package enum CheckState: Sendable {
  /// It passed.
  case done
  /// It is running.
  case busy
  /// It failed.
  case failed
}
