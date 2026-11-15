import Foundation

/// The one run that serves a VM: an exclusive lock on its folder, held for
/// as long as the run lives and let go by the kernel when it ends however
/// it ends. Held, it proves every earlier run has ended.
public final class RunnerLock: Sendable {
  private let lock: FileLock

  /// Take the lock at `file`; none while another run holds it.
  public init?(at file: URL) {
    guard let lock = FileLock(at: file) else { return nil }
    self.lock = lock
  }
}
