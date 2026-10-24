import AppKit
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("SettingsWindow")
struct SettingsWindowTests {
  @Test("the Settings window keeps its top edge when it fits a shorter pane")
  func theWindowKeepsItsTopEdge() {
    let current = CGRect(x: 100, y: 300, width: SettingsLayout.width, height: 500)
    let shorter = SettingsWindowFrame.frame(current: current, contentHeight: 320)
    #expect(shorter.maxY == current.maxY, "the top edge stays")
    #expect(shorter.height == 320)
    #expect(shorter.minX == current.minX)
    #expect(shorter.width == SettingsLayout.width)
    let screen = CGRect(x: 0, y: 40, width: 1440, height: 760)
    let taller = SettingsWindowFrame.frame(current: current, contentHeight: 900, within: screen)
    #expect(taller.height == screen.height, "never taller than the screen")
    #expect(taller.minY == screen.minY, "never past its bottom")
  }

  @Test("a pane change during a resize animates from the current height")
  func aPaneChangeMidResizeStartsFromTheCurrentFrame() throws {
    let moves = Moves()
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller, mover: moves.record) { _ in }
    let start = NSRect(x: 100, y: 300, width: SettingsLayout.width, height: 500)
    window.setFrame(start, display: false)
    // The General pane draws and says how tall it is; the window fits it, keeping its top edge.
    window.contentView?.layoutSubtreeIfNeeded()
    let general = try #require(controller.windowHeight, "the pane said how tall it draws")
    #expect(moves.frames.last == NSRect(x: 100, y: start.maxY - general, width: SettingsLayout.width, height: general))
    // Mid-resize, the window is between its frames, and the person has dragged it a little.
    let midway = NSRect(x: 140, y: 380, width: SettingsLayout.width, height: 451)
    window.setFrame(midway, display: false)
    window.contentView?.layoutSubtreeIfNeeded()
    let before = moves.frames.count
    controller.send(.paneHeight(.helper, 420))
    #expect(moves.frames.count == before, "a pane not shown never moves the window")
    controller.send(.selectTab(.helper))
    #expect(moves.frames.count == before + 1, "a resize under way never holds the next one back")
    #expect(moves.frames.last == NSRect(x: 140, y: midway.maxY - 522, width: SettingsLayout.width, height: 522))
    #expect(window.title == "Helper", "the title names the pane")
    #expect(SettingsWindowFrame.animates(isVisible: true, reduceMotion: false))
    #expect(!SettingsWindowFrame.animates(isVisible: true, reduceMotion: true), "the height jumps under Reduce Motion")
    #expect(!SettingsWindowFrame.animates(isVisible: false, reduceMotion: false))
  }
}

/// Records each frame a window is moved to, leaving the window where it is, as a resize under way would.
private final class Moves {
  /// The frames, in order.
  var frames: [NSRect] = []

  /// Records a move.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Its new frame.
  ///   - animated: Whether it would ease there.
  func record(_ window: NSWindow, _ frame: NSRect, _ animated: Bool) {
    frames.append(frame)
  }
}
