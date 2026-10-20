import Testing

@testable import AtlasKit

@Suite("TypedText")
struct TypedTextTests {
  @Test("text up to the paste limit is typed in chunks the helper accepts")
  func textIsChunked() {
    let text = String(repeating: "a", count: 2048)
    #expect(TypedText(text) == .chunks(Array(repeating: String(repeating: "a", count: 128), count: 16)))
  }

  @Test("a paste over the limit is refused, counted in Unicode characters")
  func aLongPasteIsRefused() {
    // 1025 accented letters written as a letter and its mark: 2050 characters, though 1025 graphemes.
    #expect(TypedText(String(repeating: "e\u{301}", count: 1025)) == .tooLong)
  }

  @Test("a grapheme longer than the helper's limit is refused, since it cannot be split")
  func anOversizedGraphemeIsRefused() {
    let sequence = "a" + String(repeating: "\u{301}", count: TypedText.chunkLimit)
    #expect(TypedText(sequence) == .sequenceTooLong)
  }
}
