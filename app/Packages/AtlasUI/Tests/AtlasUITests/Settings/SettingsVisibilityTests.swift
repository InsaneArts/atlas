import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("Settings surface input")
struct SettingsVisibilityTests {
  @Test("an outgoing Settings surface rejects native actions and hit testing before its fade finishes")
  func outgoingContentCannotAct() throws {
    let button = SettingsButtonProbe()
    let host = NSHostingView(rootView: SettingsNativeButton(button: button).modifier(SettingsVisibility(active: true)))
    host.frame = NSRect(x: 0, y: 0, width: SettingsLayout.contentMaximumWidth, height: SettingsLayout.headerHeight)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    button.performClick(nil)
    #expect(button.presses == 1, "the control must accept input before it retires")
    host.rootView = SettingsNativeButton(button: button)
      .modifier(SettingsVisibility(active: false))
    host.layoutSubtreeIfNeeded()
    #expect(!button.isEnabled)
    button.performClick(nil)
    #expect(button.presses == 1)
    let point = button.convert(NSPoint(x: button.bounds.midX, y: button.bounds.midY), to: host)
    #expect(host.hitTest(point) !== button)
    host.rootView = SettingsNativeButton(button: button)
      .modifier(SettingsVisibility(active: true))
    host.layoutSubtreeIfNeeded()
    button.performClick(nil)
    #expect(button.presses == 2, "an interrupted fade can retarget immediately")
  }
}
