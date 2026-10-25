import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings announcements", .serialized, AppKitState())
struct SettingsAnnouncementTests {
  @Test(
    "a failed Connect or Disconnect announces the agent and reason once across status refreshes",
    arguments: [false, true]
  )
  func agentFailuresAnnounceOnce(disconnecting: Bool) async throws {
    let commands = ScriptedCommands([
      "agents status":
        #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":false}]}"#,
      "agents install":
        #"{"ok":false,"agents":[{"agent":"claude-app","ok":false,"error":"settings file is not valid JSON"}]}"#,
      "agents remove":
        #"{"ok":false,"agents":[{"agent":"claude","ok":false,"error":"could not remove plugin"}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    var announced: [String] = []
    let announce: @MainActor (String) -> Void = { announced.append($0) }
    let host = NSHostingView(
      rootView: SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    )
    host.frame = NSRect(x: 0, y: 0, width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    if disconnecting {
      await controller.disconnect("claude")
    } else {
      await controller.connect("claude")
    }
    let problem = try #require(controller.state.agents.agents.first?.problem)
    #expect(problem.contains(disconnecting ? "Couldn't disconnect" : "Couldn't connect"))
    #expect(problem.contains(disconnecting ? "could not remove plugin" : "settings file has a mistake"))
    host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    host.layoutSubtreeIfNeeded()
    for _ in 0..<3 {
      await controller.loadAgents()
      #expect(controller.state.agents.agents.first?.problem == problem)
      host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
      host.layoutSubtreeIfNeeded()
    }
    for _ in 0..<20 { await Task.yield() }
    #expect(announced == ["Claude. \(problem)."])
    if disconnecting {
      await controller.disconnect("claude")
    } else {
      await controller.connect("claude")
    }
    host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    host.layoutSubtreeIfNeeded()
    for _ in 0..<20 { await Task.yield() }
    #expect(announced == ["Claude. \(problem).", "Claude. \(problem)."])
  }

  @Test("Down then Up announces both projects with the same folder name and distinguishes their paths")
  func sameNamedProjectsAnnounceBothMoves() async throws {
    let commands = ScriptedCommands([
      "agents status":
        #"{"ok":true,"agents":[],"projects":[{"path":"/sample/one/workspace"},{"path":"/sample/two/workspace"}]}"#
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    var announced: [String] = []
    let announce: @MainActor (String) -> Void = { announced.append($0) }
    let host = NSHostingView(
      rootView: SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    )
    host.frame = NSRect(x: 0, y: 0, width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    controller.send(.find)
    controller.send(.search("workspace"))
    let results = controller.state.search.results
    try #require(results.count == 2)
    #expect(results.allSatisfy { $0.title == "workspace" })
    host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    host.layoutSubtreeIfNeeded()
    for step in [1, -1] {
      controller.send(.moveSearchSelection(step))
      host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
      host.layoutSubtreeIfNeeded()
    }
    for _ in 0..<20 { await Task.yield() }
    #expect(
      announced == [
        "Search results available",
        "workspace, /sample/two/workspace, Agents › Projects",
        "workspace, /sample/one/workspace, Agents › Projects",
      ]
    )
  }

  @Test(
    "keyboard selection announces the result title and breadcrumb once while query edits and refreshes stay distinct"
  )
  func keyboardSearchSelectionAnnouncesOnce() async throws {
    let commands = ScriptedCommands([
      "agents status":
        #"{"ok":true,"agents":[],"projects":[{"path":"/sample/workspace/one"},{"path":"/sample/workspace/two"}]}"#
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    var announced: [String] = []
    let announce: @MainActor (String) -> Void = { announced.append($0) }
    let host = NSHostingView(
      rootView: SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    )
    host.frame = NSRect(x: 0, y: 0, width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    controller.send(.find)
    controller.send(.search("workspace"))
    let results = controller.state.search.results
    try #require(results.count == 2)
    host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    host.layoutSubtreeIfNeeded()
    #expect(announced == ["Search results available"])
    for step in [1, 1, -1, -1, 0] {
      controller.send(.moveSearchSelection(step))
      host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
      host.layoutSubtreeIfNeeded()
    }
    commands.answer("agents status", #"{"ok":true,"agents":[],"projects":[{"path":"/sample/workspace/two"}]}"#)
    await controller.loadAgents()
    #expect(controller.state.search.selection == results[1].id)
    host.rootView = SettingsView(state: controller.state, edit: { _ in }, announce: announce, send: controller.send)
    host.layoutSubtreeIfNeeded()
    for _ in 0..<20 { await Task.yield() }
    #expect(
      announced == [
        "Search results available",
        "\(results[1].title), /sample/workspace/two, \(results[1].breadcrumb)",
        "\(results[0].title), /sample/workspace/one, \(results[0].breadcrumb)",
      ]
    )
  }

  @Test(
    "one check completion or refusal is announced across Helper and Diagnostics without navigation or polling repeats",
    arguments: [nil, "The helper is in use. Try again once it's free."] as [String?]
  )
  func checkingAnnouncesOnceAcrossRoutes(refusal: String?) async throws {
    var state = PreviewStates.settingsHelper
    var announced: [String] = []
    let announce: @MainActor (String) -> Void = { announced.append($0) }
    let host = NSHostingView(rootView: SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in }))
    host.frame = NSRect(x: 0, y: 0, width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    state.helper.buttons.canCheck = false
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    state.destination = .diagnostics
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    state.helper.buttons.canCheck = true
    state.helper.readyNote = refusal.map { SettingsNote(text: $0, kind: .problem) }
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    for index in 0..<4 {
      state.destination = index.isMultiple(of: 2) ? .root(.helper) : .diagnostics
      state.helper.account.status.label = index.isMultiple(of: 2) ? "Working" : "Idle"
      host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
      host.layoutSubtreeIfNeeded()
    }
    for _ in 0..<20 { await Task.yield() }
    #expect(announced == [refusal ?? "Checked. All ready."])
  }

  @Test("routine status stays quiet even on another pane while explicit command results still announce")
  func routineStatusStaysQuiet() async throws {
    var state = PreviewStates.settingsGeneral
    var announced: [String] = []
    let told = ReleaseSignal()
    let announce: @MainActor (String) -> Void = {
      announced.append($0)
      told.open()
    }
    let host = NSHostingView(rootView: SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in }))
    host.frame = NSRect(x: 0, y: 0, width: 640, height: 640)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    state.helper.account.status.label = "Working"
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    state.helper.account.status.label = "Idle"
    state.helper.notice = SettingsNote(text: "Diagnostics copied.", kind: .quiet)
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    let result = Task { await told.wait() }
    defer { result.cancel() }
    try await result.value("the explicit command result never announced")
    #expect(announced == ["Diagnostics copied."])
  }
}
