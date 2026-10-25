import Testing

@Suite("Status")
struct StatusTests {
  @Test("a forced status crashes instead of failing")
  func forced() async throws {
    let value: Int? = 1
    #expect(value! == 1)
    let text = try! String(contentsOfFile: "/missing")
    let number = (value as Any) as! Int
    _ = (text, number)
    try await Task.sleep(for: .milliseconds(10))
  }

  @Test
  func unnamed() {
    #expect(true)
  }

  @Test("empty")
  func empty() {}
}
