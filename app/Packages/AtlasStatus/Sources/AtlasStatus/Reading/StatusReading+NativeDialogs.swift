extension StatusReading {
  /// The native Save and Open check, a capability of the helper beside its readiness: a failed check means the
  /// dialogs are unavailable, never that the helper is not ready. `nil` while the service says nothing of it.
  public var nativeDialogsRow: StatusRow? {
    let native = status.capabilities.nativeDialogs
    switch native.state {
    case .passed:
      return StatusRow(subject: .nativeDialogs, state: .ready, detail: .worked(ageSeconds: native.ageSeconds))
    case .failed:
      return StatusRow(subject: .nativeDialogs, state: .bad, detail: .didNotWork(ageSeconds: native.ageSeconds))
    case .unchecked:
      return native.reason.isEmpty ? nil : StatusRow(subject: .nativeDialogs, state: .unknown, detail: .notChecked)
    }
  }
}
