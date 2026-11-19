import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The scratch Apple's installer leaves")
struct InstallScratchTests {
  /// A scratch folder named `suffix` in `folder`, its log naming `image`.
  func scratch(_ suffix: String, in folder: URL, naming image: String) throws -> URL {
    let scratch = folder.appending(path: InstallScratch.prefix + suffix, directoryHint: .isDirectory)
    let logs = scratch.appending(path: "Logs.a1b2c3", directoryHint: .isDirectory)
    try FileManager.default
      .createDirectory(at: logs, withIntermediateDirectories: true)
    let log = "[14:33:48.9375] Build Info:\n[14:33:48.9375] \tRestore Bundle Path: \(image)\n[14:33:48.9377] done\n"
    try Data(log.utf8)
      .write(to: logs.appending(path: "log_host.txt"))
    return scratch
  }

  func exists(_ url: URL) -> Bool {
    FileManager.default
      .fileExists(atPath: url.path(percentEncoded: false))
  }

  @Test("an install's scratch is gone after it ends")
  func goneAfter() async throws {
    let folders = try Folders()
    let temporary = try folders.folder("T")
    let image = try folders.folder("RestoreImages")
      .appending(path: "27C101.ipsw")
    let path = image.path(percentEncoded: false)
    let earlier = try scratch("Earlier", in: temporary, naming: path)
    var made: [URL] = []

    try await InstallScratch.cleaning(in: temporary, image: image) {
      made.append(try scratch("Ours01", in: temporary, naming: path))
      made.append(try scratch("Theirs", in: temporary, naming: "/Users/someone/Other.ipsw"))
    }
    await #expect(throws: CocoaError.self) {
      try await InstallScratch.cleaning(in: temporary, image: image) {
        made.append(try scratch("Ours02", in: temporary, naming: path))
        throw CocoaError(.userCancelled)
      }
    }

    #expect(!exists(made[0]) && !exists(made[2]), "its own, done or failed")
    #expect(exists(made[1]), "another app's image")
    #expect(exists(earlier), "there before it began")
  }

  @Test("a link named as a scratch is never followed or removed")
  func link() throws {
    let folders = try Folders()
    let temporary = try folders.folder("T")
    let elsewhere = try folders.folder("Elsewhere")
    let image = elsewhere.appending(path: "27C101.ipsw")
    let target = try scratch("Target", in: elsewhere, naming: image.path(percentEncoded: false))
    let installing = InstallScratch(in: temporary, image: image)
    let link = temporary.appending(path: InstallScratch.prefix + "Link")
    try FileManager.default
      .createSymbolicLink(at: link, withDestinationURL: target)

    let removed = installing.remove()

    #expect(removed.isEmpty)
    #expect(exists(target))
  }
}
