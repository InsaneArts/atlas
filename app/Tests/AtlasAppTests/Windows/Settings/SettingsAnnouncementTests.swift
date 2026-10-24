import AppKit
import AsyncSupportTesting
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Settings announcements")
struct SettingsAnnouncementTests {
  @Test("one check completion is announced across Helper and Diagnostics without navigation or polling repeats")
  func checkingAnnouncesOnceAcrossRoutes() async throws {
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
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    state.destination = .root(.helper)
    state.helper.account.status.label = "Working"
    host.rootView = SettingsView(state: state, edit: { _ in }, announce: announce, send: { _ in })
    host.layoutSubtreeIfNeeded()
    for _ in 0..<20 { await Task.yield() }
    #expect(announced == ["Checked. All ready."])
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
