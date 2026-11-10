import Foundation

/// Where the questions Atlas asks the person are kept, beside the Stop in `~/Library/Caches/Atlas`, one
/// `<helper>.asks` folder for each helper, and how the app hears a change there: a seam, so tests script the changes
/// and the pictures and never touch the person's folders.
struct AskFolder {
  /// Starts hearing changes, calling `changed` on the main actor for each; the watch lasts while what it returns is
  /// kept, and nil hears nothing.
  var watch: @MainActor (_ changed: @escaping @MainActor () -> Void) -> AskFolderWatch?
  /// Reads a question's picture by its file name in an asks folder; nil when there is none.
  var picture: @Sendable (String) async -> Data?

  /// One that hears nothing and holds no pictures, for renders and tests.
  static let none = AskFolder(watch: { _ in nil }, picture: { _ in nil })

  /// The person's own: the asks folders under `home`'s `Library/Caches/Atlas`, watched with one dispatch source on
  /// that folder and one on each asks folder, so a question posts its notification at once, with no spawn and no
  /// poll.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The folder.
  static func live(home: URL) -> AskFolder {
    let root =
      home
      .appendingPathComponent("Library/Caches/Atlas", isDirectory: true)
    return AskFolder(
      watch: { changed in AskFolderWatch(root: root, changed: changed) },
      picture: { name in await read(name, under: root) }
    )
  }

  /// The asks folders under `root`, each `<helper>.asks`.
  ///
  /// - Parameter root: The folder they are kept in.
  /// - Returns: Their URLs, or none when `root` can't be read.
  nonisolated static func folders(under root: URL) -> [URL] {
    let entries = (try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? []
    return entries.filter { $0.pathExtension == "asks" }
  }

  /// A picture's bytes from whichever asks folder holds it, read off the main actor.
  ///
  /// - Parameters:
  ///   - name: The picture's file name, which never holds a folder.
  ///   - root: The folder the asks folders are kept in.
  /// - Returns: Its bytes, or nil when no asks folder holds it.
  @concurrent nonisolated static func read(_ name: String, under root: URL) async -> Data? {
    guard !name.contains("/") else { return nil }
    for folder in folders(under: root) {
      if let data = try? Data(contentsOf: folder.appendingPathComponent(name)) { return data }
    }
    return nil
  }
}
