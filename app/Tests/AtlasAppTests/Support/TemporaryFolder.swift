import Foundation

/// A folder of its own for one test, removed when the test is done with it.
struct TemporaryFolder {
  /// The folder.
  let url: URL

  /// Makes a new, empty folder.
  init() throws {
    url = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
      .appendingPathComponent("atlas-icons-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default
      .createDirectory(at: url, withIntermediateDirectories: true)
  }

  /// Removes the folder and what is in it.
  func remove() {
    try? FileManager.default
      .removeItem(at: url)
  }
}
