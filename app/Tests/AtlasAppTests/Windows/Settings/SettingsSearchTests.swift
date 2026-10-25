import AppKit
import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings search", .serialized, AppKitState())
struct SettingsSearchTests {
  @Test("unavailable preferences and searchable aliases remain discoverable from presentation metadata")
  func unavailableSettingsStayDiscoverable() {
    let commands = ScriptedCommands([:])
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    dependencies.links.opensAtLogin = { OpenAtLogin.State(isAvailable: false, isEnabled: false) }
    let controller = SettingsController(dependencies: dependencies)
    #expect(!controller.state.general.canOpenAtLogin)
    #expect(controller.state.general.pictureOptions.first { $0.id == .paused }?.isEnabled == false)
    let cases: [(String, SettingsSearchTarget)] = [
      ("startup", .startup), ("login", .startup), ("5 seconds", .cadence), ("live", .cadence), ("pause", .cadence),
      ("Claude Code", .agent("claude")), ("Claude app", .agent("claude")), ("Restart", .recover),
      ("permission", .readiness), ("accessibility", .readiness), ("screen recording", .readiness),
      ("native dialogs", .readiness), ("doctor", .readiness), ("diagnostics", .readiness),
      ("update", .updates), ("version", .updates),
    ]
    for (query, target) in cases {
      controller.send(.search(query))
      #expect(controller.state.search.results.contains { $0.id == target }, "\(query)")
    }
    #expect(commands.ran.isEmpty)
  }

  @Test("loaded project names and paths are searchable and later readings remove stale results atomically")
  func projectResultsFollowLoadedData() async {
    let commands = ScriptedCommands(["agents status": Self.projects])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    controller.send(.search("workspace"))
    let before = controller.state.search.results
    #expect(before.map(\.id) == [.project("/sample/workspace/one"), .project("/sample/workspace/two")])
    controller.send(.moveSearchSelection(1))
    let selected = controller.state.search.selection
    commands.answer("agents status", Self.projects)
    await controller.loadAgents()
    #expect(controller.state.search.selection == selected)
    commands.answer("agents status", #"{"ok":true,"agents":[],"projects":[{"path":"/sample/workspace/one"}]}"#)
    await controller.loadAgents()
    #expect(controller.state.search.results.map(\.id) == [.project("/sample/workspace/one")])
    #expect(controller.state.search.selection == .project("/sample/workspace/one"))
    controller.send(.openSearchResult(.project("/sample/workspace/two")))
    #expect(controller.state.search.isActive, "a stale click cannot navigate")
    controller.send(.search("one"))
    #expect(controller.state.search.results.contains { $0.id == .project("/sample/workspace/one") })
    commands.answer("agents status", #"{"ok":true,"agents":[],"projects":[]}"#)
    await controller.loadAgents()
    #expect(controller.state.search.results.isEmpty && controller.state.search.selection == nil)
  }

  @Test("clear and Escape's empty edit restore the underlying page without adding a history entry")
  func clearingSearchPreservesHistory() {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    controller.send(.navigate(.diagnostics))
    #expect(controller.state.sidebarSelection == .helper)
    let history = controller.session.navigation
    controller.send(.find)
    let requests = controller.state.search.focusRequests
    controller.send(.search("nothing matches"))
    #expect(controller.state.title == "Search Results")
    #expect(controller.state.sidebarSelection == nil)
    #expect(controller.state.search.results.isEmpty)
    controller.send(.openSelectedSearchResult)
    #expect(controller.session.navigation == history)
    controller.send(.search(""))
    #expect(!controller.state.search.isActive)
    #expect(controller.state.location == .diagnostics)
    #expect(controller.state.sidebarSelection == .helper)
    #expect(controller.state.title == "Readiness & Diagnostics")
    #expect(controller.state.search.focusRequests == requests)
    #expect(controller.session.navigation == history)
    controller.send(.search(" \n\t "))
    #expect(!controller.state.search.isActive)
    controller.send(.search("Recover the helper"))
    controller.send(.openSelectedSearchResult)
    #expect(controller.state.reveal?.target == .recover)
    controller.send(.search(""))
    #expect(controller.state.reveal?.target == .recover, "an unchanged native field update retains target focus")
    controller.send(.find)
    let afterFind = controller.state.search.focusRequests
    controller.send(.search("helper"))
    controller.send(.search(""))
    #expect(controller.state.reveal == nil, "clearing must not replay an old target's focus request")
    #expect(controller.state.search.focusRequests == afterFind)
  }

  @Test("arrows select within bounds and Return on Recover and Log Out visits once without asking or running")
  func returnOnlyNavigatesHelperResults() async throws {
    for (query, target) in [("Recover the helper", SettingsSearchTarget.recover), ("Log out the helper", .logOut)] {
      let commands = ScriptedCommands([:])
      var dependencies = SettingsController.scripted(commands: commands).dependencies
      var savedRoots: [String] = []
      dependencies.preferences.saveTab = { savedRoots.append($0) }
      let controller = SettingsController(dependencies: dependencies)
      controller.send(.search(query))
      let keyboard = SettingsSearchKeyboard(controller)
      defer { keyboard.close() }
      let editor = try await keyboard.focus()
      #expect(controller.state.search.selection == target)
      try await keyboard.press("\u{f700}", code: 126)
      #expect(controller.state.search.selection == target)
      #expect(keyboard.window.firstResponder === editor)
      try await keyboard.press("\r", code: 36)
      #expect(controller.state.location == .root(.helper))
      #expect(controller.state.sidebarSelection == .helper)
      #expect(controller.state.reveal?.target == target)
      #expect(keyboard.window.firstResponder is NSView, "the revealed target takes keyboard focus")
      #expect(keyboard.window.firstResponder !== editor, "Return transfers focus to the revealed target")
      #expect(controller.state.alert == nil && commands.ran.isEmpty)
      #expect(savedRoots.isEmpty, "a search result never writes a preference")
      #expect(!controller.state.search.isActive)
      #expect(controller.session.navigation.entries == [.root(.general), .root(.helper)])
      try await keyboard.press("\r", code: 36)
      #expect(controller.state.alert == nil && commands.ran.isEmpty)
      controller.send(.back)
      #expect(controller.state.location == .root(.general))
      controller.send(.forward)
      #expect(controller.state.location == .root(.helper))
    }
  }

  @Test("only plain arrows navigate results while modified arrows retain native text editing")
  func modifiedArrowsStayWithTheField() async throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    controller.send(.search("helper"))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let editor = try await keyboard.focus()
    let results = controller.state.search.results
    try #require(results.count > 1)
    try await keyboard.press("\u{f701}", code: 125)
    #expect(controller.state.search.selection == results[1].id)
    try await keyboard.press("\u{f700}", code: 126)
    #expect(controller.state.search.selection == results[0].id)
    for modifiers: NSEvent.ModifierFlags in [.command, .option, .shift, .control, [.command, .shift]] {
      for (characters, code): (String, UInt16) in [("\u{f700}", 126), ("\u{f701}", 125)] {
        editor.setSelectedRange(NSRange(location: 3, length: 0))
        try await keyboard.press(characters, code: code, modifiers: modifiers)
        #expect(controller.state.search.selection == results[0].id, "\(modifiers)")
        #expect(keyboard.window.firstResponder === editor)
        if modifiers == .command {
          #expect(editor.selectedRange().location == (code == 126 ? 0 : editor.string.utf16.count))
        }
      }
    }
  }

  @Test("Escape clears the query first and releases the search field on its second press")
  func escapeHasTwoStages() async throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    controller.send(.search("helper"))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let editor = try await keyboard.focus()
    try await keyboard.press("\u{1b}", code: 53)
    #expect(!controller.state.search.isActive)
    #expect(keyboard.window.firstResponder === editor)
    try await keyboard.press("\u{1b}", code: 53)
    #expect(keyboard.window.firstResponder !== editor)
  }

  @Test("Return on Forget only reveals the project and a root selection clears search normally")
  func returnNeverForgetsAProject() async throws {
    let commands = ScriptedCommands(["agents status": Self.projects])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    controller.send(.search("Forget workspace"))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    _ = try await keyboard.focus()
    for _ in controller.state.search.results { try await keyboard.press("\u{f701}", code: 125) }
    #expect(controller.state.search.selection == .project("/sample/workspace/two"))
    try await keyboard.press("\r", code: 36)
    #expect(controller.state.location == .projects)
    #expect(controller.state.reveal?.target == .project("/sample/workspace/two"))
    #expect(controller.state.alert == nil)
    #expect(commands.ran == [Action.agentsStatus.arguments], "opening loaded data never refreshes it")
    controller.send(.search("helper"))
    controller.send(.navigate(.root(.screen)))
    #expect(!controller.state.search.isActive && controller.state.location == .root(.screen))
    #expect(controller.session.navigation.entries == [.root(.general), .projects, .root(.screen)])
  }

  @Test("typing, selection and helper polls never write preferences, refresh agents, run commands or steal focus")
  func searchHasNoWritesOrPollDrivenFocus() {
    let commands = ScriptedCommands([:])
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    var writes = 0
    var reads = 0
    dependencies.preferences.saveTab = { _ in writes += 1 }
    dependencies.preferences.saveNotify = { _ in writes += 1 }
    dependencies.links.setOpenAtLogin = { _ in writes += 1 }
    dependencies.links.choosePicture = { _ in writes += 1 }
    dependencies.links.readStatus = { reads += 1 }
    dependencies.links.copyDiagnostics = { writes += 1 }
    let status = dependencies.links.status().status
    let polls = CallCount()
    dependencies.links.status = { (status, polls.count.isMultiple(of: 2)) }
    let controller = SettingsController(dependencies: dependencies)
    controller.send(.find)
    for query in ["h", "he", "helper", "recover", "log out", "forget", "helper"] { controller.send(.search(query)) }
    controller.send(.moveSearchSelection(1))
    let selection = controller.state.search.selection
    let requests = controller.state.search.focusRequests
    let history = controller.session.navigation
    for _ in 0..<10 {
      polls.count += 1
      #expect(controller.state.search.selection == selection)
      #expect(controller.state.search.focusRequests == requests)
    }
    controller.send(.search(""))
    #expect(writes == 0 && reads == 0 && commands.ran.isEmpty)
    #expect(controller.session.navigation == history)
  }

  @Test("an alert blocks search editing, selection, Find and result opens underneath it")
  func alertsKeepSearchUnavailable() {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    controller.send(.search("helper"))
    let before = controller.state.search
    controller.send(.askRecover)
    controller.send(.search("new query"))
    controller.send(.moveSearchSelection(1))
    controller.send(.find)
    controller.send(.openSelectedSearchResult)
    #expect(controller.state.search == before)
    #expect(controller.state.location == .root(.general))
  }

  /// Two loaded paths with stable names, without reading any real projects.
  private static let projects =
    #"{"ok":true,"agents":[],"projects":[{"path":"/sample/workspace/one"},{"path":"/sample/workspace/two"}]}"#
}
