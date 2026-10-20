import Foundation
import Testing

@testable import AtlasKit

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

  @Test("a status reply other than a 200, such as a busy service's 503, is refused, never read as a status")
  func onlyA200IsAStatus() throws {
    let url = try #require(URL(string: "http://127.0.0.1:8765/abc/status.json"))
    let busy = try #require(HTTPURLResponse(url: url, statusCode: 503, httpVersion: nil, headerFields: nil))
    let refusal = Data(#"{"ok": false, "error": "Viewer is busy"}"#.utf8)
    #expect(throws: StatusRefused("Viewer is busy")) { try ViewerClient.status(refusal, response: busy) }
    let answered = try #require(HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil))
    let reply = Data(#"{"ready": true, "overview": {"mode": "observing"}}"#.utf8)
    #expect(try ViewerClient.status(reply, response: answered).overview.mode == .observing)
  }
}
