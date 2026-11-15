import Darwin
import Foundation

/// An exclusive lock on a file, held for as long as this lives and let go
/// by the kernel when its process ends however it ends.
public final class FileLock: Sendable {
  private let descriptor: Int32

  /// Take the lock at `file`; none while another holds it.
  public init?(at file: URL) {
    let opened = open(file.path(percentEncoded: false), O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard opened >= 0 else { return nil }
    guard flock(opened, LOCK_EX | LOCK_NB) == 0 else {
      close(opened)
      return nil
    }
    descriptor = opened
  }

  deinit {
    close(descriptor)
  }
}
