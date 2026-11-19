import Foundation
import Testing
import os

@testable import AtlasService

@Suite("ViewerClient+Desktops")
struct ViewerClientDesktopsTests {
  /// What `desktops.json` lists: Second Login, viewed, and a VM.
  private static let listed = Data(
    #"""
    {"ok": true, "viewed": "a1", "desktops": [
      {"id": "a1", "key": "user", "name": "main", "kind": "user", "default": true, "viewed": true,
       "standing": "idle"},
      {"id": "b2", "key": "vm-b2", "name": "work", "kind": "vm", "default": false, "viewed": false,
       "standing": null, "facts": {"can": ["stop", "maintain", "remove"], "disk": null}}
    ]}
    """#.utf8
  )

  /// Each request a client made, as its route and the desktop it named.
  private final class Asked: Sendable {
    /// Every request so far, in order.
    private let asked = OSAllocatedUnfairLock(initialState: [(route: String, desktop: String?)]())

    /// Every request so far, in order.
    var all: [(route: String, desktop: String?)] { asked.withLock { $0 } }

    /// Keeps `request`.
    ///
    /// - Parameter request: The request.
    func keep(_ request: URLRequest) {
      let components = request.url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false) }
      let desktop = components?.queryItems?
        .first { $0.name == "desktop" }?
        .value
      let route = request.url?.pathComponents
        .dropFirst(2)
        .joined(separator: "/")
      asked.withLock { $0.append((route ?? "", desktop)) }
    }
  }

  /// A client answering as a service serving two desktops, every request kept in `asked`.
  ///
  /// - Parameter asked: Where each request is kept.
  /// - Returns: The client.
  /// - Throws: When the capability URL is refused, which a fixed one never is.
  private func client(_ asked: Asked) throws -> ViewerClient {
    let secret = UUID().uuidString
    let session = URLSession.scripted(
      secret: secret,
      answer: .each { request in
        asked.keep(request)
        let body =
          switch request.url?.lastPathComponent {
          case "desktops.json": Self.listed
          case "status.json": Data(#"{"ready": true, "desktopKey": "user"}"#.utf8)
          case "acquire": Data(#"{"ok": true, "lease": "lease"}"#.utf8)
          default: Data(#"{"ok": true}"#.utf8)
          }
        return .reply(status: 200, body: body)
      }
    )
    return try #require(ViewerClient("http://127.0.0.1:8765/\(secret)/", appSecret: "app", session: session))
  }

  @Test("the desktops list reads each desktop, its standing and what a VM can do")
  func theListReadsEachDesktop() async throws {
    let reading = try client(Asked())
    let list = try await reading.desktops()
    #expect(list.viewed == "a1")
    #expect(list.desktops.map(\.name) == ["main", "work"])
    let main = try #require(list.desktops.first)
    #expect(main.kind == .user)
    #expect(main.isDefault && main.isViewed)
    #expect(main.standing == .idle)
    let work = try #require(list.desktops.last)
    #expect(work.kind == .vm)
    #expect(work.key == "vm-b2")
    #expect(!work.isViewed)
    #expect(work.standing == nil)
    #expect(work.can == ["stop", "maintain", "remove"])
  }

  @Test("a request names no desktop until the service names the one it views")
  func noDesktopIsNamedUntilTheServiceNamesOne() async throws {
    let asked = Asked()
    let client = try client(asked)
    _ = try await client.status()
    #expect(asked.all.map(\.desktop) == [nil])
  }

  @Test("the two-desktop app reads status and controls the viewed desktop")
  func twoDesktopsReadAndControlTheViewedOne() async throws {
    let asked = Asked()
    let client = try client(asked)
    _ = try await client.desktops()
    _ = try await client.status()
    _ = try await client.acquire(frame: "frame")
    try await client.view("b2")
    _ = try await client.status()
    _ = try await client.acquire(frame: "frame")
    try await client.release(lease: "lease")
    let named = asked.all
      .dropFirst()
      .map { "\($0.route) \($0.desktop ?? "none")" }
    #expect(
      named == [
        "status.json a1", "control/acquire a1", "view a1",
        "status.json b2", "control/acquire b2", "control/release b2",
      ]
    )
    #expect(client.viewed.id == "b2")
  }

  @Test("a desktop refused for viewing leaves the one viewed")
  func aRefusedViewLeavesTheOneViewed() async throws {
    let secret = UUID().uuidString
    let refusal = Data(#"{"ok": false, "code": "config-invalid", "error": "This Atlas serves another desktop"}"#.utf8)
    let session = URLSession.scripted(
      secret: secret,
      answer: .each { request in
        request.url?.lastPathComponent == "desktops.json"
          ? .reply(status: 200, body: Self.listed) : .reply(status: 409, body: refusal)
      }
    )
    let client = try #require(ViewerClient("http://127.0.0.1:8765/\(secret)/", appSecret: "app", session: session))
    _ = try await client.desktops()
    await #expect(throws: ServiceError("This Atlas serves another desktop", code: "config-invalid")) {
      try await client.view("elsewhere")
    }
    #expect(client.viewed.id == "a1")
  }
}
