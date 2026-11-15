import Foundation
import Testing

@testable import AtlasService

@Suite("ServiceConnection")
struct ServiceConnectionTests {
  @Test("a connection not attached refuses every request, saying Atlas is reconnecting")
  func aDetachedConnectionRefuses() async {
    let connection = ServiceConnection()
    await #expect(throws: ServiceError.detached) { try await connection.status() }
    await #expect(throws: ServiceError.detached) { try await connection.activity() }
    #expect(connection.incarnation == nil)
  }

  @Test("a connection names the service start it is attached to, and none once detached")
  func aConnectionNamesItsStart() throws {
    let connection = ServiceConnection()
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
    connection.attach(client, incarnation: "first")
    #expect(connection.incarnation == "first")
    connection.detach()
    #expect(connection.incarnation == nil)
    connection.attach(client, incarnation: "second")
    #expect(connection.incarnation == "second")
  }

  @Test("a service of another version is told to every request until the app attaches again")
  func anotherVersionIsToldUntilAttached() async throws {
    let connection = ServiceConnection()
    let another = ServiceError("Atlas in the menu bar is from another version. Restart Atlas.", code: "protocol")
    connection.detach(because: another)
    await #expect(throws: another) { try await connection.status() }
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
    connection.attach(client, incarnation: "first")
    connection.detach()
    await #expect(throws: ServiceError.detached) { try await connection.status() }
  }

  @Test("nothing listening at the socket reads as Atlas not running")
  func nothingListeningIsNotRunning() throws {
    let folder = FileManager.default.temporaryDirectory
      .appending(path: UUID().uuidString)
    try FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    defer {
      try? FileManager.default
        .removeItem(at: folder)
    }
    #expect(throws: ServiceError("Atlas isn't running.", code: "not-running")) {
      try ServiceAttachment(path: folder.appending(path: "attach.sock"), service: folder.appending(path: "atlas"))
    }
  }
}
