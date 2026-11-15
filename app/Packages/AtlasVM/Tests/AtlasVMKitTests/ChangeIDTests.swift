import Foundation
import Testing

@testable import AtlasVMKit

@Suite("Change IDs")
struct ChangeIDTests {
  @Test("a change's ID is 48 lowercase hexadecimal digits, and nothing else reads as one")
  func digits() throws {
    let id = String(repeating: "a1", count: 24)
    #expect(ChangeID(id)?.text == id)
    for notOne in ["", String(repeating: "AB", count: 24), String(repeating: "a", count: 47), "../" + id] {
      #expect(ChangeID(notOne) == nil, "\(notOne)")
    }
    let decoded = try JSONDecoder()
      .decode([ChangeID].self, from: Data(#"["\#(id)"]"#.utf8))
    #expect(decoded == [ChangeID(id)])
    #expect(throws: DecodingError.self) {
      try JSONDecoder()
        .decode([ChangeID].self, from: Data(#"["nope"]"#.utf8))
    }
  }
}
