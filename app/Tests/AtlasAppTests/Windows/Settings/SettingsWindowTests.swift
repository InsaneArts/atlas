import AppKit
import AtlasCommandsTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("SettingsWindow")
struct SettingsWindowTests {
  @Test("a first opening with no saved frame centers before enabling autosave")
  func aFirstOpeningCenters() {
    let name = "Settings-first-opening-\(UUID().uuidString)"
    defer { NSWindow.removeFrame(usingName: name) }
    #expect(UserDefaults.standard.string(forKey: "NSWindow Frame \(name)") == nil)
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    var centered = false
    let window = SettingsWindow(
      controller: controller,
      autosaveName: name,
      center: { window in
        #expect(window.frameAutosaveName.isEmpty, "centering precedes any automatic frame writes")
        #expect(UserDefaults.standard.string(forKey: "NSWindow Frame \(name)") == nil)
        centered = true
      },
      closed: { _ in }
    )
    #expect(centered)
    #expect(window.frameAutosaveName == name)
  }

  @Test("a restored narrow frame grows to the minimum while retaining its top edge")
  func anOldFrameGrowsToTheMinimum() {
    let current = CGRect(x: 100, y: 300, width: 680, height: 400)
    let restored = SettingsWindowFrame.frame(current: current)
    #expect(restored.size == CGSize(width: SettingsLayout.minimumWidth, height: SettingsLayout.minimumHeight))
    #expect(restored.maxY == current.maxY)
    #expect(restored.minX == current.minX)
  }

  @Test("restored frames fit every edge of the usable screen including a screen smaller than the minimum")
  func restoredFramesStayOnScreen() {
    let visible = CGRect(x: -1440, y: 40, width: 1440, height: 860)
    for frame in [
      CGRect(x: -1600, y: -200, width: 960, height: 700),
      CGRect(x: -300, y: 600, width: 1000, height: 800),
      CGRect(x: -1500, y: 0, width: 2000, height: 1200),
    ] {
      let restored = SettingsWindowFrame.frame(current: frame, within: visible)
      #expect(visible.contains(restored))
      #expect(restored.width >= SettingsLayout.minimumWidth)
      #expect(restored.height >= SettingsLayout.minimumHeight)
    }
    let small = CGRect(x: 0, y: 25, width: 800, height: 500)
    #expect(SettingsWindowFrame.frame(current: visible, within: small) == small)
    #expect(SettingsWindowFrame.minimum(within: small) == small.size)
  }

  @Test("a usable user-resized frame stays unchanged when validated")
  func aUserSizeIsPreserved() {
    let visible = CGRect(x: 0, y: 40, width: 1600, height: 1000)
    let resized = CGRect(x: 120, y: 80, width: 1120, height: 820)
    #expect(SettingsWindowFrame.frame(current: resized, within: visible) == resized)
  }

  @Test("navigation and live status change the title without resizing the Settings window")
  func navigationKeepsTheWindowFrame() {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller) { _ in }
    window.setFrame(NSRect(x: 100, y: 100, width: 1040, height: 760), display: false)
    let resized = window.frame
    window.contentView?.layoutSubtreeIfNeeded()
    controller.send(.selectTab(.helper))
    #expect(window.title == "Settings — Helper")
    controller.session.checking = true
    controller.say("A longer reported status does not change the viewport.", in: .helper)
    window.contentView?.layoutSubtreeIfNeeded()
    #expect(window.frame == resized)
    controller.send(.back)
    #expect(window.title == "Settings — General")
    #expect(window.frame == resized)
    controller.send(.forward)
    #expect(window.title == "Settings — Helper")
    #expect(window.frame == resized)
    #expect(window.styleMask.contains(.resizable))
    #expect(!window.styleMask.contains(.miniaturizable))
    #expect(window.frameAutosaveName == SettingsWindow.autosaveName)
  }
}
