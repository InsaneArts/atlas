import Foundation

/// The standard error collected on another thread.
///
/// `@unchecked Sendable` because a dispatch group orders its one write
/// before its one read.
final class CollectedOutput: @unchecked Sendable {
  // Written once before `reading.leave()`, read once after `wait()`.
  var data = Data()
}
