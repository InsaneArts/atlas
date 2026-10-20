import Foundation
import Testing

@testable import AtlasKit

@Suite("InputAction")
struct InputActionTests {
  @Test(
    "each input action encodes exactly its type and its own fields, as the service's input endpoint takes them",
    arguments: [
      (
        .pointer(phase: .down, x: 1.5, y: 2, button: .left, count: 1, modifiers: ["shift"]),
        #"{"type": "pointer", "phase": "down", "x": 1.5, "y": 2, "button": "left", "count": 1, "modifiers": ["shift"]}"#
      ),
      (.move(x: 3.5, y: 4), #"{"type": "move", "x": 3.5, "y": 4}"#),
      (.scroll(x: 5, y: 6, dx: -1, dy: 3), #"{"type": "scroll", "x": 5, "y": 6, "dx": -1, "dy": 3}"#),
      (.type(text: "hi"), #"{"type": "type", "text": "hi"}"#),
      (.key("cmd+v"), #"{"type": "key", "key": "cmd+v"}"#),
      (.wait(milliseconds: 50), #"{"type": "wait", "ms": 50}"#),
    ] as [(InputAction, String)]
  )
  func encoding(action: InputAction, payload: String) throws {
    let encoder = JSONEncoder()
    let encoded = try #require(try JSONSerialization.jsonObject(with: encoder.encode(action)) as? NSDictionary)
    let expected = try #require(try JSONSerialization.jsonObject(with: Data(payload.utf8)) as? NSDictionary)
    #expect(encoded == expected)
  }

  @Test("only pointer moves and plain moves may be replaced while queued")
  func isMove() {
    #expect(InputAction.move(x: 0, y: 0).isMove)
    #expect(InputAction.pointer(phase: .move, x: 0, y: 0, button: .left, count: 1, modifiers: []).isMove)
    #expect(!InputAction.pointer(phase: .down, x: 0, y: 0, button: .left, count: 1, modifiers: []).isMove)
    #expect(!InputAction.key("a").isMove)
  }
}
