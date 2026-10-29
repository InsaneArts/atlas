import Testing

@testable import AtlasUI

@Suite("ViewerSessionsState")
struct ViewerSessionsStateTests {
  /// A session's row.
  ///
  /// - Parameter key: Its key, which is its name too.
  /// - Returns: The row.
  private static func row(_ key: String) -> ViewerSessionRow {
    ViewerSessionRow(id: key, icon: .tile(.text(key)), name: key, line: "")
  }

  /// All sessions, one session needing the person, one working, one idle, then one that ended, chosen and listed
  /// while Ended is folded.
  private static let sessions = ViewerSessionsState(
    icons: [],
    liveCount: 3,
    groups: [
      ViewerSessionGroup(.needsYou, rows: [row("n")]),
      ViewerSessionGroup(.working, rows: [row("w")]),
      ViewerSessionGroup(.idle, rows: [row("i")]),
      ViewerSessionGroup(.ended, rows: [row("e")]),
    ],
    endedCount: 4
  )

  @Test("↑ and ↓ step through sessions in the order they are drawn")
  func theArrowsStepInTheOrderDrawn() {
    let sessions = Self.sessions
    #expect(sessions.choices == [.all, .one("n"), .one("w"), .one("i"), .one("e")])
    #expect(sessions.choice(1, from: .all) == .one("n"), "Needs you comes first")
    #expect(sessions.choice(1, from: .one("i")) == .one("e"), "the ended session drawn under Ended follows")
    #expect(sessions.choice(-1, from: .one("n")) == .all)
  }

  @Test("the arrow keys hold at the first and last rows drawn")
  func theArrowsHoldAtTheEnds() {
    let sessions = Self.sessions
    #expect(sessions.choice(-1, from: .all) == .all)
    #expect(sessions.choice(1, from: .one("e")) == .one("e"))
  }

  @Test("a choice with no row steps from the top")
  func aChoiceWithNoRowStepsFromTheTop() {
    #expect(Self.sessions.choice(1, from: .one("gone")) == .one("n"))
  }

  @Test("a section's rows are those it draws, and a section with none draws none")
  func aSectionsRowsAreThoseItDraws() {
    #expect(Self.sessions.rows(.working).map(\.id) == ["w"])
    let quiet = ViewerSessionsState(icons: [], liveCount: 0, groups: [])
    #expect(quiet.rows(.idle).isEmpty)
  }
}
