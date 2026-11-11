/// What tells the setup window that something it waits for may have changed: the live ``SetupChanges``, or a test's.
@MainActor
protocol SetupWatching: AnyObject {
  /// One element for each moment something may have changed, at most one kept while the window is busy; ends once
  /// ``stop()`` is called.
  var changes: AsyncStream<Void> { get }

  /// Watches `folders` too, in place of those watched before: one that does not exist yet is watched for through the
  /// nearest folder above it that does.
  ///
  /// - Parameter folders: The folders' paths.
  func watch(folders: [String])

  /// Stops every watch and ends ``changes``.
  func stop()
}
