import Foundation
import Testing

/// A reply's response from the viewer service, with status `code`.
///
/// - Parameter code: The reply's HTTP status.
/// - Returns: The response.
/// - Throws: When the response cannot be made.
func reply(_ code: Int) throws -> HTTPURLResponse {
  let url = try #require(URL(string: "http://127.0.0.1:8765/abc/apps.json"))
  return try #require(HTTPURLResponse(url: url, statusCode: code, httpVersion: nil, headerFields: nil))
}
