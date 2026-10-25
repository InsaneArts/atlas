import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerClient")
struct ViewerClientTests {
  @Test(
    "only a local http capability URL with a port and a trailing slash is accepted",
    arguments: [
      ("http://127.0.0.1:8765/abc/\n", true),
      ("http://example.com:8765/abc/", false),
      ("https://127.0.0.1:8765/abc/", false),
      ("http://127.0.0.1/abc/", false),
      ("http://127.0.0.1:8765/abc", false),
    ]
  )
  func capabilityURL(text: String, accepted: Bool) {
    #expect((ViewerClient(text) != nil) == accepted)
  }

  @Test("a request carries the service's own origin")
  func origin() throws {
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
    let request = try client.request("control/release", body: ["lease": "x"])
    #expect(request.url?.absoluteString == "http://127.0.0.1:8765/abc/control/release")
    #expect(request.value(forHTTPHeaderField: "Origin") == "http://127.0.0.1:8765")
    #expect(request.httpMethod == "POST")
  }

  @Test("the capability URL's secret never prints, however the client is printed")
  func theSecretNeverPrints() throws {
    let client = try #require(ViewerClient("http://127.0.0.1:8765/s3cret/"))
    var dumped = ""
    dump(client, to: &dumped)
    for printed in [String(describing: client), String(reflecting: client), dumped] {
      #expect(!printed.contains("s3cret"))
      #expect(printed.contains(ViewerClient.redacted))
    }
  }

  @Test("only a busy service's 503 is waited out; any other reply but a 200 fails")
  func onlyA200IsAStatus() throws {
    let url = try #require(URL(string: "http://127.0.0.1:8765/abc/status.json"))
    let busy = try #require(HTTPURLResponse(url: url, statusCode: 503, httpVersion: nil, headerFields: nil))
    let refusal = Data(#"{"ok": false, "error": "Viewer is busy"}"#.utf8)
    #expect(throws: StatusRefused("Viewer is busy")) { try ViewerClient.status(refusal, response: busy) }
    let forbidden = try #require(HTTPURLResponse(url: url, statusCode: 403, httpVersion: nil, headerFields: nil))
    let denied = Data(#"{"ok": false, "error": "Forbidden"}"#.utf8)
    #expect(throws: ServiceError.self) { try ViewerClient.status(denied, response: forbidden) }
    let answered = try #require(HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil))
    let reply = Data(#"{"ready": true, "overview": {"mode": "observing"}}"#.utf8)
    #expect(try ViewerClient.status(reply, response: answered).overview.mode == .observing)
  }

  @Test("a screenshot request keeps the capability URL, percent-encodes the name and asks for the size wanted")
  func screenshotRequest() throws {
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
    let thumb = try client.screenshotRequest(name: "1 a&size=full+b/\u{e9}.png", full: false)
    let encoded = "1%20a%26size%3Dfull%2Bb%2F%C3%A9.png"
    #expect(thumb.url?.absoluteString == "http://127.0.0.1:8765/abc/screenshot.png?name=\(encoded)&size=thumb")
    let full = try client.screenshotRequest(name: "1700000005000000000-capture.png", full: true)
    #expect(full.url?.query == "name=1700000005000000000-capture.png&size=full")
  }

  @Test("an apps reply that is not a 200 is the service's error with its code, never an empty list")
  func appsRefused() throws {
    #expect(throws: ServiceError("The helper is not running.", code: "unavailable")) {
      try ViewerClient.apps(appsUnreachable, response: reply(503))
    }
  }
}
