import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerClient, the launch's secret")
struct ViewerClientSecretTests {
  /// A secret as the service writes it.
  static let secret = String(repeating: "0123456789abcdef", count: 4)

  @Test("the secret is read from its line, and any other line is none")
  func theSecretIsReadFromItsLine() {
    #expect(ViewerClient.appSecret(fromLine: "secret \(Self.secret)\n") == Self.secret)
    #expect(ViewerClient.appSecret(fromLine: "secret \(Self.secret.uppercased())") == nil, "lowercase only")
    #expect(ViewerClient.appSecret(fromLine: "secret abc") == nil, "64 digits")
    #expect(ViewerClient.appSecret(fromLine: "viewer") == nil)
    #expect(ViewerClient.appSecret(fromLine: "secret \(Self.secret) more") == nil)
  }

  @Test("every request carries the secret, reads and changes alike, and a client given none sends none")
  func everyRequestCarriesIt() throws {
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/", appSecret: Self.secret))
    for request in [
      try client.request("status.json"),
      try client.request("asks/answer", body: ["id": "a1"]),
      client.request(to: client.base.appendingPathComponent("asks.json")),
      try client.screenshotRequest(name: "shot.png", full: false),
    ] {
      #expect(request.value(forHTTPHeaderField: "X-Atlas-App-Secret") == Self.secret)
    }
    let without = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
    #expect(try without.request("status.json").value(forHTTPHeaderField: "X-Atlas-App-Secret") == nil)
  }

  @Test("the secret never prints, however the client is printed")
  func theSecretNeverPrints() throws {
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/", appSecret: Self.secret))
    var dumped = ""
    dump(client, to: &dumped)
    for printed in [String(describing: client), String(reflecting: client), dumped] {
      #expect(!printed.contains(Self.secret))
    }
  }

  @Test("a change refused for want of the secret says how to get it back, never the secret, and doesn't crash")
  func aMissingSecretSaysWhatToDo() async throws {
    let refusal = Data(#"{"ok": false, "code": "permission", "error": "Only Atlas.app may do this"}"#.utf8)
    let path = UUID().uuidString
    let without = try #require(
      ViewerClient(
        "http://127.0.0.1:8765/\(path)/",
        session: .scripted(secret: path, answer: .reply(status: 403, body: refusal))
      )
    )
    await #expect(throws: ServiceError(ViewerClient.noSecret, code: "permission")) {
      try await without.answer("a1", with: .deny)
    }
    await #expect(throws: ServiceError(ViewerClient.noSecret, code: "permission")) { try await without.asks() }
    let with = try #require(
      ViewerClient(
        "http://127.0.0.1:8765/\(path)/",
        appSecret: Self.secret,
        session: .scripted(secret: path, answer: .reply(status: 403, body: refusal))
      )
    )
    do {
      try await with.answer("a1", with: .deny)
      Issue.record("a refusal answered")
    } catch {
      #expect(!String(describing: error).contains(Self.secret))
      #expect(!error.localizedDescription.contains(Self.secret))
    }
  }
}
