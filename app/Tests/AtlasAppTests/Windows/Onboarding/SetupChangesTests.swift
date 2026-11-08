import AsyncSupportTesting
import Foundation
import Testing

@testable import AtlasApp

@Suite("SetupChanges", .serialized)
@MainActor
struct SetupChangesTests {
  /// A fresh folder of its own under the temporary folder.
  private static func folder() throws -> URL {
    let files = FileManager.default
    let folder = files.temporaryDirectory
      .appending(path: "setup-changes-\(UUID().uuidString)")
    try make(folder)
    return folder
  }

  /// Makes the folder at `url`, and the folders above it.
  private static func make(_ url: URL) throws {
    let files = FileManager.default
    try files.createDirectory(at: url, withIntermediateDirectories: true)
  }

  /// Removes what is at `url`, if anything.
  private static func remove(_ url: URL) throws {
    let files = FileManager.default
    try files.removeItem(at: url)
  }

  /// Writes an empty file at `url`.
  private static func touch(_ url: URL) throws {
    let empty = Data()
    try empty.write(to: url)
  }

  /// Starts counting `watcher`'s changes.
  private static func count(_ watcher: SetupChanges) -> CountedChanges {
    let counted = CountedChanges()
    // Unstructured, so the test goes on while it counts; it ends with the stream.
    Task { @MainActor in
      for await _ in watcher.changes { counted.changes += 1 }
      counted.ended = true
    }
    return counted
  }

  @Test("a change in a watched folder is reported")
  func aChangeInAWatchedFolder() async throws {
    let folder = try Self.folder()
    defer { try? Self.remove(folder) }
    let watcher = SetupChanges()
    defer { watcher.stop() }
    let counted = Self.count(watcher)
    watcher.watch(folders: [folder.path])
    try Self.touch(folder.appending(path: "accepted.json"))
    try await settle("the write was never reported") { counted.changes >= 1 }
  }

  @Test("a folder that does not exist yet is watched through its parent, then itself once made")
  func aFolderMadeLater() async throws {
    let parent = try Self.folder()
    defer { try? Self.remove(parent) }
    let folder = parent.appending(path: "helper")
    let watcher = SetupChanges()
    defer { watcher.stop() }
    let counted = Self.count(watcher)
    watcher.watch(folders: [folder.path])
    try Self.make(folder)
    try await settle("making the folder was never reported") { counted.changes >= 1 }
    let made = counted.changes
    try Self.touch(folder.appending(path: "501.sock"))
    try await settle("a change inside the new folder was never reported") { counted.changes > made }
  }

  @Test("a folder replaced is still watched")
  func aReplacedFolder() async throws {
    let parent = try Self.folder()
    defer { try? Self.remove(parent) }
    let folder = parent.appending(path: "Atlas-accept-502")
    try Self.make(folder)
    let watcher = SetupChanges()
    defer { watcher.stop() }
    let counted = Self.count(watcher)
    watcher.watch(folders: [folder.path])
    try Self.remove(folder)
    try await settle("removing the folder was never reported") { counted.changes >= 1 }
    try Self.make(folder)
    try await settle("making it again was never reported") { counted.changes >= 2 }
    let remade = counted.changes
    try Self.touch(folder.appending(path: "accepted.json"))
    try await settle("a write in the new folder was never reported") { counted.changes > remade }
  }

  @Test("a folder replaced at once, with no pause between, is still watched")
  func aFolderReplacedAtOnce() async throws {
    let parent = try Self.folder()
    defer { try? Self.remove(parent) }
    let folder = parent.appending(path: "Atlas-accept-502")
    try Self.make(folder)
    let watcher = SetupChanges()
    defer { watcher.stop() }
    let counted = Self.count(watcher)
    watcher.watch(folders: [folder.path])
    try Self.remove(folder)
    try Self.make(folder)
    try await settle("the replacement was never reported") { counted.changes >= 1 }
    // The watcher's own repairs settle; asked again as a reading does, it watches the folder now there.
    watcher.watch(folders: [folder.path])
    let before = counted.changes
    try Self.touch(folder.appending(path: "accepted.json"))
    try await settle("a write in the replaced folder was never reported") { counted.changes > before }
  }

  @Test("stopping ends the changes and the folders' watches")
  func stoppingEndsEverything() async throws {
    let folder = try Self.folder()
    defer { try? Self.remove(folder) }
    let watcher = SetupChanges()
    let counted = Self.count(watcher)
    watcher.watch(folders: [folder.path])
    watcher.stop()
    try await settle("the changes never ended") { counted.ended }
    watcher.watch(folders: [folder.path])
    try Self.touch(folder.appending(path: "late"))
    #expect(counted.changes == 0)
  }

  @Test("the nearest folder that exists stands in for one that does not")
  func nearestExisting() throws {
    let folder = try Self.folder()
    defer { try? Self.remove(folder) }
    #expect(SetupChanges.nearestExisting(folder.path) == folder.path)
    #expect(SetupChanges.nearestExisting(folder.appending(path: "a/b").path) == folder.path)
    let file = folder.appending(path: "file")
    try Self.touch(file)
    #expect(SetupChanges.nearestExisting(file.appending(path: "c").path) == folder.path, "a file is no folder")
  }
}
