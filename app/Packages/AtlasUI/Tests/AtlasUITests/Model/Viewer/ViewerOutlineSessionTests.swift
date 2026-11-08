import Testing

@testable import AtlasUI

@Suite("ViewerOutlineSession")
struct ViewerOutlineSessionTests {
  /// A session of Claude Code's in a project.
  ///
  /// - Parameter project: Its project; none by default.
  /// - Returns: The session.
  private static func session(project: String = "") -> ViewerOutlineSession {
    ViewerOutlineSession(
      id: "claude",
      icon: nil,
      agent: "Claude Code",
      project: project,
      summary: "TextEdit · 9 actions",
      time: "10:21–now",
      runs: []
    )
  }

  @Test("a session's row puts its project first, then its agent")
  func theRowPutsTheProjectFirst() {
    let session = Self.session(project: "atlas-t2-r2")
    #expect(session.title == "atlas-t2-r2")
    #expect(session.byline == "Claude Code")
    #expect(session.label == "atlas-t2-r2, Claude Code, TextEdit · 9 actions, 10:21–now")
  }

  @Test("a session with no project is named by its agent alone")
  func aSessionWithNoProjectIsNamedByItsAgent() {
    let session = Self.session()
    #expect(session.title == "Claude Code")
    #expect(session.byline.isEmpty)
  }
}
