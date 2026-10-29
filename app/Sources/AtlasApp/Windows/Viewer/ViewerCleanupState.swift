import AtlasService

/// Where this window's cleanups of sessions' work stand: the one read or running now, the one whose reply never came,
/// and what each session's last one did. One cleanup runs at a time, so either of the first two holds every
/// session's button.
struct ViewerCleanupState: Equatable {
  /// The session whose cleanup is being read or runs now; nil while none is.
  var running: String?
  /// The cleanup whose apply got no reply and may still run, until `cleanup.json` shows it ended; nil while none is.
  var unanswered: ViewerUnansweredCleanup?
  /// What each session's last cleanup from this window did, by its key, which Details lists above its button.
  var receipts: [String: CleanupReply] = [:]

  /// Whether a cleanup is read, runs or may still run now, which holds Clean Up This Session's Work… for every
  /// session.
  var isBusy: Bool { running != nil || unanswered != nil }

  /// Whether `key`'s own cleanup is read, runs or may still run now, which shows its button's spinner.
  ///
  /// - Parameter key: The session's key.
  /// - Returns: Whether its cleanup is under way.
  func isBusy(for key: String) -> Bool {
    running == key || unanswered?.session == key
  }

  /// Forgets what the last cleanups did for sessions the list no longer names, as once they are past the seven days
  /// it keeps.
  ///
  /// - Parameter keys: The keys of the sessions the list names.
  mutating func keepReceipts(for keys: Set<String>) {
    receipts = receipts.filter { keys.contains($0.key) }
  }
}
