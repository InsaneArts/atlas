import Darwin
import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A VM's folder")
struct VMFolderTests {
  @Test("a VM's folder is named by its desktop alone and made private with its share and changes")
  func made() throws {
    let folders = try Folders()
    #expect(VMFolder(in: folders.root, desktop: "../elsewhere") == nil)
    #expect(VMFolder(in: folders.root, desktop: "0123456789ABCDEF01234567") == nil)
    let folder = try #require(VMFolder(in: folders.root, desktop: "0123456789abcdef01234567"))

    try folder.prepare()

    for made in [folder.root, folder.share, folder.changes] {
      try VMFolder.checkPrivate(made)
    }
    #expect(folder.station.lastPathComponent == "station")
  }

  @Test("a folder others may read, by its mode or an access entry, or a link in a folder's place, is refused")
  func refused() throws {
    let folders = try Folders()
    let open = try folders.folder("open")
    chmod(open.path(percentEncoded: false), 0o755)
    #expect(throws: VMFolderError.notPrivate(open)) { try VMFolder.checkPrivate(open) }

    let link = folders.root.appending(path: "link")
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: try folders.folder("real"))
    #expect(throws: VMFolderError.notAFolder(link)) { try VMFolder.checkPrivate(link) }

    let shared = try folders.folder("shared")
    let granting = Process()
    granting.executableURL = URL(filePath: "/bin/chmod")
    granting.arguments = ["+a", "everyone allow list,search", shared.path(percentEncoded: false)]
    try granting.run()
    granting.waitUntilExit()
    #expect(granting.terminationStatus == 0)
    #expect(throws: VMFolderError.notPrivate(shared)) { try VMFolder.checkPrivate(shared) }
  }
}
