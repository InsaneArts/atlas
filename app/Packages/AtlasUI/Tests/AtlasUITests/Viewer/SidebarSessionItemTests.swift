import SwiftUI
import Testing

@testable import AtlasUI

@Suite("SidebarSessionItem")
struct SidebarSessionItemTests {
  /// Claude Code's row.
  private static let claude = ViewerSessionRow(id: "c", icon: .tile(.text("C")), name: "Claude Code", line: "Acting")

  /// The sessions with Claude Code under `section`, alone.
  ///
  /// - Parameter section: Its section.
  /// - Returns: The sessions.
  private static func sessions(_ section: ViewerSessionSection) -> ViewerSessionsState {
    ViewerSessionsState(icons: [], liveCount: 1, groups: [ViewerSessionGroup(section, rows: [claude])])
  }

  @Test("a session moving from Working to Idle keeps its one row's identity, so the row glides")
  func aMovedRowKeepsItsIdentity() {
    let working = SidebarSessionItem.items(Self.sessions(.working), reduceMotion: false)
    let idle = SidebarSessionItem.items(Self.sessions(.idle), reduceMotion: false)
    #expect(working.map(\.id) == ["heading.working", "row.c"])
    #expect(idle.map(\.id) == ["heading.idle", "row.c"])
  }

  @Test("under Reduce Motion a moved row is a new one in its new section, so it fades instead of travelling")
  func underReduceMotionAMovedRowFades() {
    let working = SidebarSessionItem.items(Self.sessions(.working), reduceMotion: true)
    let idle = SidebarSessionItem.items(Self.sessions(.idle), reduceMotion: true)
    #expect(working.last?.id == "working.c")
    #expect(idle.last?.id == "idle.c")
  }

  @Test("Reduce Motion removes sidebar layout interpolation while ordinary updates retain it")
  func reducedMotionHasNoLayoutAnimation() {
    #expect(SidebarSessionList.layoutMotion(reduceMotion: true) == nil)
    #expect(SidebarSessionList.layoutMotion(reduceMotion: false) == Motion.state)
  }

  @Test("a line's changing words leave the list's order alone, so only a move animates it")
  func aChangingLineLeavesTheOrderAlone() {
    var later = Self.sessions(.working)
    later.groups[0].rows[0].line = "Scrolled in Opie"
    let before = SidebarSessionItem.items(Self.sessions(.working), reduceMotion: false)
      .map(\.id)
    #expect(SidebarSessionItem.items(later, reduceMotion: false).map(\.id) == before)
  }
}
