import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings navigation")
struct SettingsNavigationTests {
  @Test("an empty history stays empty at both boundaries and accepts its first visit")
  func anEmptyHistoryHasNoVisits() {
    var history = SettingsNavigation()
    history.back()
    history.forward()
    #expect(history.destination == nil)
    #expect(history.entries.isEmpty)
    #expect(!history.canGoBack && !history.canGoForward)
    history.visit(.root(.helper))
    #expect(history.destination == .root(.helper))
    #expect(!history.canGoBack && !history.canGoForward)
  }

  @Test("reselecting the current destination preserves history including its forward branch")
  func reselectingDoesNotDuplicateAVisit() {
    var history = SettingsNavigation(initial: .root(.general))
    history.visit(.root(.helper))
    history.visit(.diagnostics)
    history.back()
    let before = history
    history.visit(.root(.helper))
    #expect(history == before)
    history.forward()
    #expect(history.destination == .diagnostics)
  }

  @Test("Back and Forward move through visits once and stop at their boundaries")
  func historyStopsAtItsBoundaries() {
    var history = SettingsNavigation(initial: .root(.general))
    history.visit(.root(.helper))
    history.visit(.diagnostics)
    let visits = history.entries
    history.forward()
    #expect(history.destination == .diagnostics)
    history.back()
    #expect(history.destination == .root(.helper))
    history.back()
    history.back()
    #expect(history.destination == .root(.general))
    #expect(!history.canGoBack && history.canGoForward)
    history.forward()
    history.forward()
    history.forward()
    #expect(history.destination == .diagnostics)
    #expect(history.canGoBack && !history.canGoForward)
    #expect(history.entries == visits)
  }

  @Test("a different destination after Back discards the forward branch")
  func aNewVisitReplacesTheForwardBranch() {
    var history = SettingsNavigation(initial: .root(.general))
    history.visit(.root(.helper))
    history.visit(.diagnostics)
    history.back()
    history.visit(.projects)
    #expect(history.entries == [.root(.general), .root(.helper), .projects])
    #expect(history.destination?.root == .agents)
    #expect(!history.canGoForward)
    history.back()
    #expect(history.destination?.root == .helper)
  }

  @Test("detail destinations save and select their owning root rather than a detail identifier")
  func detailsBelongToTheirRoots() {
    #expect(SettingsDestination.diagnostics.root == .helper)
    #expect(SettingsDestination.projects.root == .agents)
    for root in SettingsRoot.allCases {
      #expect(SettingsDestination.root(root).root == root)
      #expect(SettingsRoot(savedValue: root.rawValue) == root)
    }
  }

  @Test("shipped saved pane values restore the same content while invalid values fall back to General")
  func savedPanesKeepTheirMeaning() {
    for pane in SettingsTab.allCases {
      let root = SettingsRoot(savedValue: pane.rawValue)
      #expect(root.pane == pane)
      #expect(root == SettingsRoot(pane: pane))
    }
    for invalid: String? in [nil, "", "missing", "diagnostics", "projects", "Helper"] {
      #expect(SettingsRoot(savedValue: invalid) == .general)
    }
  }
}
