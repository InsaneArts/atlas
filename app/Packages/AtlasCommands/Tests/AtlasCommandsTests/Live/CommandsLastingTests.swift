import Darwin
import Foundation
import Testing

@testable import AtlasCommands

@Suite("Commands lasting")
struct CommandsLastingTests {
  @Test("a lasting command's reply and its words are read from its private files, which go once it ends")
  func aLastingCommandIsReadFromItsFiles() async throws {
    let folder = Self.folder()
    let files = FileManager.default
    defer { try? files.removeItem(at: folder) }
    let script = #"echo '{"progress": "Making the VM"}' >&2; echo '{"ok": true, "args": "'"$*"'"}'"#
    let reply = try await withScriptedAtlas(script) { atlas in
      await atlas.runLasting(["desktop", "add", "vm"], keeping: folder)
    }
    struct Echo: Decodable { let args: String }
    #expect(reply.ok)
    #expect(reply.decode(Echo.self)?.args == "--json desktop add vm")
    let attributes = try files.attributesOfItem(atPath: folder.path)
    #expect(attributes[.posixPermissions] as? Int == 0o700, "the folder is this account's alone")
    #expect(try files.contentsOfDirectory(atPath: folder.path).isEmpty, "its files go once it ends")
  }

  @Test("cancelling the wait for a lasting command never stops the command")
  func cancellingTheWaitLeavesTheCommandRunning() async throws {
    let folder = Self.folder()
    let files = FileManager.default
    try files.createDirectory(at: folder, withIntermediateDirectories: true)
    defer { try? files.removeItem(at: folder) }
    // The command waits on the gate, which only the test opens, then says it went on through the second pipe.
    let gate = folder.appending(path: "gate")
    let back = folder.appending(path: "back")
    #expect(mkfifo(gate.path, 0o600) == 0 && mkfifo(back.path, 0o600) == 0)
    let script = "cat '\(gate.path)' > /dev/null; echo on > '\(back.path)'; echo '{\"ok\": true}'"
    let (outcome, said) = try await withScriptedAtlas(script) { atlas in
      let waiting = Task { await atlas.runLasting(["desktop", "add", "vm"], keeping: folder) }
      waiting.cancel()
      let outcome = await waiting.value
      // Each open waits for the command at the pipe's other end, so it is the sentinel that it still runs.
      let opened = FileHandle(forWritingAtPath: gate.path)
      try opened?.write(contentsOf: Data("go\n".utf8))
      try opened?.close()
      let said = FileHandle(forReadingAtPath: back.path)?
        .readDataToEndOfFile()
      return (outcome, said)
    }
    #expect(!outcome.ok)
    #expect(outcome.code == "still-running")
    #expect(said == Data("on\n".utf8), "the command went on past its wait")
  }

  /// A new folder's place in this account's temporary folder.
  ///
  /// - Returns: The folder's URL, not made yet.
  private static func folder() -> URL {
    FileManager.default.temporaryDirectory
      .appending(path: "atlas-lasting-\(UUID().uuidString)")
  }
}
