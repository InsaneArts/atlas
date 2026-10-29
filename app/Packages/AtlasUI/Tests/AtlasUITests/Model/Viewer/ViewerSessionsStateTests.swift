import Testing

@testable import AtlasUI

@Suite("ViewerSessionsState")
struct ViewerSessionsStateTests {
  /// All sessions, two live sessions, then one that ended.
  private static let column = ViewerSessionsState(
    icons: [],
    live: [
      ViewerSessionRow(id: "a", icon: .tile(.text("A")), name: "A", state: "Acting"),
      ViewerSessionRow(id: "b", icon: .tile(.text("B")), name: "B", state: "Idle"),
    ],
    ended: [ViewerSessionRow(id: "c", icon: .tile(.text("C")), name: "C", state: "Ended")]
  )

  @Test("the arrow keys step through All sessions, the live sessions and the ended ones, top to bottom")
  func theArrowsStepTopToBottom() {
    let column = Self.column
    #expect(column.choice(1, from: .all) == .one("a"))
    #expect(column.choice(1, from: .one("b")) == .one("c"), "the ended sessions follow the live ones")
    #expect(column.choice(-1, from: .one("a")) == .all)
  }

  @Test("the arrow keys hold at the column's first and last rows")
  func theArrowsHoldAtTheEnds() {
    let column = Self.column
    #expect(column.choice(-1, from: .all) == .all)
    #expect(column.choice(1, from: .one("c")) == .one("c"))
  }

  @Test("a choice with no row steps from the top")
  func aChoiceWithNoRowStepsFromTheTop() {
    #expect(Self.column.choice(1, from: .one("gone")) == .one("a"))
  }
}
