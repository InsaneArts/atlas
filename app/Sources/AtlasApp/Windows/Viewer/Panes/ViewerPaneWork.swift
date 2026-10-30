/// The pane and host-session reading tasks and their cancellation generations.
struct ViewerPaneWork {
  /// Numbers each start and stop of a pane's reading, so a stopped one ends and its late reply is dropped.
  var generation = 0
  /// The pane's reading loop.
  var reading: Task<Void, Never>?
  /// Numbers each start and stop of the sessions' list's reading, so a stopped one ends and its late reply is dropped.
  var sessionsGeneration = 0
  /// The sessions' list's reading loop, while the window wants the list.
  var sessions: Task<Void, Never>?
  /// Whether the sessions task is polling live sessions rather than discovering history once.
  var sessionsPolling = false
}
