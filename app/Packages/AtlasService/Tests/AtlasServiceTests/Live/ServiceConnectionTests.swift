import Foundation
import Testing
import os

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

@Suite("ServiceConnection desktops")
struct ServiceConnectionDesktopsTests {
  @Test("a reply for the desktop viewed before the app viewed another is dropped")
  func aReplyForTheDesktopViewedBeforeIsDropped() async throws {
    let connection = ServiceConnection()
    let secret = UUID().uuidString
    let session = URLSession.scripted(
      secret: secret,
      answer: .each { request in
        switch request.url?.lastPathComponent {
        case "status.json":
          // The app views another desktop while this reply is on its way.
          connection.viewed.view("b2")
          return .reply(status: 200, body: Data(#"{"ready": true, "desktopKey": "user"}"#.utf8))
        default:
          return .reply(status: 404, body: Data(#"{"ok": false, "error": "Unknown operation"}"#.utf8))
        }
      }
    )
    let client = try #require(ViewerClient("http://127.0.0.1:8765/\(secret)/", appSecret: "app", session: session))
    connection.attach(client, incarnation: "first")
    connection.viewed.view("a1")
    defer { connection.detach() }

    await #expect(throws: ServiceError.otherDesktop) { try await connection.status() }
    #expect(connection.viewed.id == "b2")
  }

  /// A service listing one desktop whose ID changes once, as Second Login's does when its account is replaced, and
  /// saying so on its feed once.
  private final class Replacing: Sendable {
    /// The ID of the desktop it serves now, and whether its feed told the change.
    private let state = OSAllocatedUnfairLock(initialState: (id: "old", replaced: false, told: false))

    /// Serves a desktop of another ID from now on.
    func replace() {
      state.withLock { $0 = (id: "new", replaced: true, told: false) }
    }

    /// The answer to `request`.
    ///
    /// - Parameter request: The request.
    /// - Returns: Its answer.
    func answer(_ request: URLRequest) -> ScriptedURLProtocol.Answer {
      let components = request.url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false) }
      let query = components?.queryItems ?? []
      let named = query.first { $0.name == "desktop" }?
        .value
      let (id, tell) = state.withLock { state in
        let tell = state.replaced && !state.told
        if request.url?.lastPathComponent == "events.json" && tell { state.told = true }
        return (state.id, tell)
      }
      let body: String
      switch request.url?.lastPathComponent {
      case "desktops.json":
        body = #"{"ok": true, "viewed": "\#(id)", "desktops": [{"id": "\#(id)", "kind": "user", "viewed": true}]}"#
      case "events.json" where !query.contains { $0.name == "feed" }:
        body = #"{"ok": true, "reset": {"feed": "f", "head": 0}}"#
      case "events.json" where tell:
        body =
          #"{"ok": true, "feed": "f", "head": 1, "events": [{"feed": "f", "seq": 1, "at": 0, "desktop": "\#(id)", "kind": "desktops"}]}"#
      case "events.json":
        body = #"{"ok": true, "feed": "f", "head": 1, "events": [], "poll": true}"#
      case _ where named != nil && named != id:
        let refused = #"{"ok": false, "code": "config-invalid", "error": "Another desktop"}"#
        return .reply(status: 409, body: Data(refused.utf8))
      case "status.json":
        body = #"{"ready": true}"#
      case "acquire":
        body = #"{"ok": true, "lease": "lease"}"#
      default:
        body = #"{"ok": true}"#
      }
      return .reply(status: 200, body: Data(body.utf8))
    }
  }

  /// Waits until the feed `connection` follows tells a change of the desktops after the count `seen`: the binding is
  /// read again before that is told.
  ///
  /// - Parameters:
  ///   - connection: The connection.
  ///   - seen: The count noted before.
  private func told(_ connection: ServiceConnection, since seen: UInt64) async {
    await connection.changes.pause(.seconds(30), on: [.desktops], since: seen, clock: ContinuousClock())
  }

  @Test("a desktop replaced while one is listed is viewed from the feed, and status, control and Resume recover")
  func aReplacedDesktopIsViewedFromTheFeed() async throws {
    let service = Replacing()
    let secret = UUID().uuidString
    let session = URLSession.scripted(secret: secret, answer: .each { service.answer($0) })
    let client = try #require(ViewerClient("http://127.0.0.1:8765/\(secret)/", appSecret: "app", session: session))
    let connection = ServiceConnection()
    let attached = connection.changes.count(of: [.desktops])
    connection.attach(client, incarnation: "first")
    defer { connection.detach() }
    await told(connection, since: attached)
    #expect(connection.viewed.id == "old")
    _ = try await connection.status()

    let before = connection.changes.count(of: [.desktops])
    service.replace()
    await told(connection, since: before)
    #expect(connection.viewed.id == "new")

    _ = try await connection.status()
    #expect(try await connection.acquire(frame: "frame") == "lease")
    try await connection.setStopped(false)
    #expect(connection.incarnation == "first", "never attached again")
  }
}
