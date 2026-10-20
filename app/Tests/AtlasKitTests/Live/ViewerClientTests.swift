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
}
