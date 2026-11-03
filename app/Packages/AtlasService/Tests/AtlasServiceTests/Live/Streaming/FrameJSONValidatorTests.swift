import Foundation
import Testing

@testable import AtlasService

@Suite("Frame metadata validator")
struct FrameJSONValidatorTests {
  /// What validating `text` gives: its top-level kinds, or its refusal.
  private static func validate(_ text: String) -> Result<[String: FrameJSONKind], FramePipeError> {
    Result { () throws(FramePipeError) in try FrameJSONValidator.validate(Data(text.utf8)) }
  }

  @Test("top-level values report their kinds, with integers only in plain decimal syntax")
  func kinds() throws {
    let kinds = try Self.validate(
      #"{"a":1,"b":-1,"c":1.0,"d":1e2,"e":"x","f":true,"g":null,"h":{},"i":[1,{"j":2}]}"#
    )
    .get()
    #expect(kinds["a"] == .integer)
    #expect(kinds["b"] == .number)
    #expect(kinds["c"] == .number)
    #expect(kinds["d"] == .number)
    #expect(kinds["e"] == .string)
    #expect(kinds["f"] == .bool)
    #expect(kinds["g"] == .null)
    #expect(kinds["h"] == .object)
    #expect(kinds["i"] == .array)
    #expect(kinds["j"] == nil, "nested keys are not top-level")
  }

  @Test("a key twice in one object is refused however it is escaped")
  func duplicates() {
    #expect(Self.validate(#"{"a":1,"a":2}"#) == .failure(.invalid))
    #expect(Self.validate(#"{"a":1,"a":2}"#) == .failure(.invalid))
    #expect(Self.validate(#"{"x":{"a":1,"a":2}}"#) == .failure(.invalid))
    #expect((try? Self.validate(#"{"a":{"a":1}}"#).get()) != nil, "the same key in another object is fine")
  }

  @Test("invalid text, escapes and surrogates are refused")
  func invalidText() {
    for text in [
      #"{"a":"\x"}"#, #"{"a":"\ud800"}"#, #"{"a":"\udc00"}"#, #"{"a":"\ud800A"}"#, "{\"a\":\"\u{1}\"}",
      #"{"a":01}"#, #"{"a":1.}"#, #"{"a":-}"#, #"{"a":tru}"#, #"{"a":1}x"#, #"{"a":1,}"#, "[1]", "\"a\"", "",
    ] {
      #expect(Self.validate(text) == .failure(.invalid), "\(text)")
    }
    #expect(Self.validate(#"{"a":"😀"}"#).map { $0["a"] } == .success(.string))
    #expect(throws: FramePipeError.invalid) { try FrameJSONValidator.validate(Data([0x7B, 0xFF, 0x7D])) }
  }

  @Test("depth, key count, key length, string length and size stay within the pipe's bounds")
  func bounds() {
    let deep =
      String(repeating: #"{"a":"#, count: FramePipeLimits.depth) + "1"
      + String(repeating: "}", count: FramePipeLimits.depth)
    #expect((try? Self.validate(deep).get()) != nil)
    let deeper =
      String(repeating: #"{"a":"#, count: FramePipeLimits.depth + 1) + "1"
      + String(repeating: "}", count: FramePipeLimits.depth + 1)
    #expect(Self.validate(deeper) == .failure(.oversized))
    let many = (0...FramePipeLimits.keys).map { #""k\#($0)":0"# }.joined(separator: ",")
    #expect(Self.validate("{\(many)}") == .failure(.oversized))
    let key = String(repeating: "k", count: FramePipeLimits.key + 1)
    #expect(Self.validate(#"{"\#(key)":0}"#) == .failure(.oversized))
    let value = String(repeating: "v", count: FramePipeLimits.string + 1)
    #expect(Self.validate(#"{"a":"\#(value)"}"#) == .failure(.oversized))
    let padded = "{" + String(repeating: " ", count: FramePipeLimits.metadata) + "}"
    #expect(Self.validate(padded) == .failure(.oversized))
  }
}
