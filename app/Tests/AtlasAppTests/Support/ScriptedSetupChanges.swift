@testable import AtlasApp

/// A ``SetupWatching`` a test drives: it reports a change when told to, and keeps the folders it was asked to watch.
@MainActor
final class ScriptedSetupChanges: SetupWatching {
  let changes: AsyncStream<Void>
  private let sink: AsyncStream<Void>.Continuation
  /// The folders asked to be watched, each time they changed.
  private(set) var watched: [[String]] = []
  /// Whether the window stopped it.
  private(set) var stopped = false

  init() {
    (changes, sink) = AsyncStream.makeStream(of: Void.self, bufferingPolicy: .bufferingNewest(1))
  }

  func watch(folders: [String]) {
    if watched.last != folders { watched.append(folders) }
  }

  func stop() {
    stopped = true
    sink.finish()
  }

  /// Something may have changed.
  func change() {
    sink.yield()
  }
}
