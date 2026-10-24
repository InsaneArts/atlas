import Testing

@Suite("Status")
struct StatusTests {
  @Test("a ready status reads as ready, and != compares by value")
  func readyReadsAsReady() throws {
    let value: Int? = 1
    let unwrapped = try #require(value)
    #expect(unwrapped != 2)
    #expect(!false)
    // A comment may say value! or try! without a report.
    #expect("a string with value! inside" != "")
  }
}
