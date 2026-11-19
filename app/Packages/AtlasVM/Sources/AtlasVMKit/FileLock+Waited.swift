import Foundation

extension FileLock {
  /// The lock at `file`, waited for a second at a time while another holds
  /// it.
  public static func waited(at file: URL) async throws -> FileLock {
    while true {
      if let lock = FileLock(at: file) { return lock }
      try await Task.sleep(for: .seconds(1))
    }
  }
}
