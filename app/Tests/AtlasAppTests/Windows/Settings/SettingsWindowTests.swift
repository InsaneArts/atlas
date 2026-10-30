import AppKit
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("SettingsWindow", .serialized, AppKitState())
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
    #expect(moves.animated.allSatisfy { !$0 }, "a window not on screen lands at once")
    #expect(SettingsWindowFrame.animates(isVisible: true, reduceMotion: false))
    #expect(!SettingsWindowFrame.animates(isVisible: true, reduceMotion: true), "the height jumps under Reduce Motion")
    #expect(!SettingsWindowFrame.animates(isVisible: false, reduceMotion: false))
  }

  @Test(
    "the real mover eases the window to its frame, and a pane changed mid-resize eases on from where it is",
    .disabled(
      if: ProcessInfo.processInfo.environment["CI"] != nil,
      "It samples AppKit's own animation in real time, which a slow shared runner can't catch mid-motion"
    )
  )
  @MainActor
  func theMoverEasesAndRetargets() throws {
    // A window never ordered in: AppKit's animator still steps its frame, and nothing shows on the desktop.
    let window = NSWindow(
      contentRect: NSRect(x: 100, y: 300, width: SettingsLayout.width, height: 500),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    let start = window.frame
    let taller = NSRect(x: start.minX, y: start.maxY - 700, width: start.width, height: 700)
    SettingsWindow.move(window, to: taller, animated: true)
    #expect(window.frame != taller, "it eases there, never lands at once")
    try Self.run(until: "the resize never got half way") { window.frame.height >= 600 }
    let midway = window.frame
    #expect(midway.height < taller.height, "caught mid-resize")
    let shorter = NSRect(x: start.minX, y: midway.maxY - 420, width: start.width, height: 420)
    SettingsWindow.move(window, to: shorter, animated: true)
    var heights = [window.frame.height]
    try Self.run(until: "the second resize never landed") {
      heights.append(window.frame.height)
      return window.frame == shorter
    }
    // Started again from the first frame, it would be back at 500 or below at once.
    #expect(heights.first.map { $0 > start.height && $0 <= taller.height } == true, "it eases on from where it is")
    #expect(zip(heights, heights.dropFirst()).allSatisfy { $0 >= $1 }, "straight down to the new height")
    #expect(heights.count > 2, "in steps, not at once")
    SettingsWindow.move(window, to: start, animated: false)
    #expect(window.frame == start, "without easing it lands at once")
  }

  /// Turns the main run loop, a hundredth of a second at a time, until `isDone` holds: AppKit's animator steps a
  /// window's frame on it. Not a pause: it returns as soon as the frame is there, and fails past a second's bound.
  ///
  /// - Parameters:
  ///   - failure: What failed, should the bound pass first.
  ///   - isDone: Whether the wait is over.
  /// - Throws: When the bound passes first.
  @MainActor
  private static func run(until failure: String, _ isDone: () -> Bool) throws {
    let bound = Date(timeIntervalSinceNow: 1)
    while !isDone() {
      try #require(Date() < bound, Comment(rawValue: failure))
      let loop = RunLoop.main
      loop.run(until: Date(timeIntervalSinceNow: 0.01))
    }
  }
}

/// Records each frame a window is moved to, leaving the window where it is, as a resize under way would.
private final class Moves {
  /// The frames, in order.
  var frames: [NSRect] = []
  /// Whether each move would ease, in order.
  var animated: [Bool] = []

  /// Records a move.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Its new frame.
  ///   - animated: Whether it would ease there.
  func record(_ window: NSWindow, _ frame: NSRect, _ animated: Bool) {
    frames.append(frame)
    self.animated.append(animated)
  }
}
