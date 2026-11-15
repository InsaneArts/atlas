import Foundation

@testable import AtlasVMKit

/// A private folder of its own for one test, removed when the test is done.
final class Folders {
  let root: URL

  init() throws {
    root = FileManager.default.temporaryDirectory
      .appending(
        path: "avm-\(UUID().uuidString.prefix(8))",
        directoryHint: .isDirectory
      )
    try VMFolder.makePrivate(root)
  }

  deinit {
    try? FileManager.default
      .removeItem(at: root)
  }

  /// A new private folder in it.
  func folder(_ name: String) throws -> URL {
    let folder = root.appending(path: name, directoryHint: .isDirectory)
    try VMFolder.makePrivate(folder)
    return folder
  }
}
