import Testing

@testable import AtlasControl

@Suite("String+TypedChunks")
struct StringTypedChunksTests {
  let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"
  let accent = "e\u{301}"

  @Test("typed text is chunked without splitting a grapheme")
  func graphemes() {
    #expect("abcdef".typedChunks(size: 4) == ["abcd", "ef"])
    #expect("ab\(family)".typedChunks(size: 4) == ["ab", family])
    #expect("abc\(accent)d".typedChunks(size: 4) == ["abc", "\(accent)d"])
    let long = String(repeating: "\(accent)\(family)", count: 40)
    let chunks = long.typedChunks(size: TypedText.chunkSize)
    #expect(chunks.allSatisfy { $0.unicodeScalars.count <= TypedText.chunkSize })
    #expect(chunks.joined() == long)
    #expect(family.typedChunks(size: 2) == [family], "an oversized grapheme stays whole")
    #expect("".typedChunks(size: 4).isEmpty)
  }
}
