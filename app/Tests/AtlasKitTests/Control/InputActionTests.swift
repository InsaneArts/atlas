import Foundation
import Testing

@testable import AtlasKit

@Suite("InputAction")
struct InputActionTests {
  @Test("input actions encode as the service's input endpoint takes them")
  func encoding() throws {
    let actions: [InputAction] = [
      .pointer(phase: .down, x: 1, y: 2, button: .left, count: 1, modifiers: ["shift"]),
      .type(text: "hi"), .key("cmd+v"), .wait(milliseconds: 50),
    ]
    let json = try #require(
      try JSONSerialization.jsonObject(with: JSONEncoder().encode(actions)) as? [[String: Any]]
    )
    #expect(json[0]["type"] as? String == "pointer")
    #expect(json[0]["phase"] as? String == "down")
    #expect(json[1]["text"] as? String == "hi")
    #expect(json[2]["key"] as? String == "cmd+v")
    #expect(json[3]["ms"] as? Int == 50)
  }

  @Test("only pointer moves and plain moves may be replaced while queued")
  func isMove() {
    #expect(InputAction.move(x: 0, y: 0).isMove)
    #expect(InputAction.pointer(phase: .move, x: 0, y: 0, button: .left, count: 1, modifiers: []).isMove)
    #expect(!InputAction.pointer(phase: .down, x: 0, y: 0, button: .left, count: 1, modifiers: []).isMove)
    #expect(!InputAction.key("a").isMove)
  }
}
