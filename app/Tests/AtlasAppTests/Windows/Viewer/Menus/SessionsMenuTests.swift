import AppKit
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("SessionsMenu")
struct SessionsMenuTests {
  /// Answers the menu's items in a test.
  final class Target: NSObject {}

  @Test("ended sessions follow under their header and nothing in the menu acts in the helper")
  func endedSessionsFollowUnderTheirHeader() {
    let item = { (key: String, title: String) in
      ViewerSessionsMenuItem(key: key, title: title, subtitle: "Idle", icon: SessionIcons.icon(.claude))
    }
    let value = ViewerSessionsMenu(
      chosen: .one("b"),
      live: [item("a", "Claude Code · understudy"), item("b", "Codex · atlas-landing")],
      ended: [item("c", "Pi · notes-sync")]
    )
    let menu = SessionsMenu.menu(value, target: Target(), scale: 2)
    let titles = menu.items.map { $0.isSeparatorItem ? "—" : $0.title }
    #expect(
      titles == ["All Sessions", "—", "Claude Code · understudy", "Codex · atlas-landing", "Ended", "Pi · notes-sync"]
    )
    let ticked = menu.items
      .filter { $0.state == .on }
      .map(\.title)
    #expect(ticked == ["Codex · atlas-landing"])
    let choosing = menu.items.filter { !$0.isSeparatorItem && !$0.isSectionHeader }
    #expect(choosing.allSatisfy { $0.action == #selector(ViewerMenuActions.chooseSession(_:)) })
    #expect(choosing.map { $0.representedObject as? String } == [nil, "a", "b", "c"])
    let sessions = choosing.dropFirst()
    #expect(sessions.allSatisfy { $0.image != nil })
  }

  @Test("the sessions menu opens under its button, lined up with its leading edge")
  func theMenuLinesUpWithItsButtonsLeadingEdge() {
    let button = CGRect(x: 410, y: 11, width: 170, height: 30)
    #expect(ViewerWindow.menuOrigin(leadingBelow: button) == NSPoint(x: 410, y: 41 + ViewerWindow.menuGap))
  }
}
