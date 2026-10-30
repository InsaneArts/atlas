import AppKit
import SwiftUI

@testable import AtlasUI

/// An offscreen panel control row with a local Return equivalent; events never reach the desktop.
@MainActor
final class PanelButtonHost {
  private let window: NSWindow
  private let hosting = NSHostingView(rootView: AnyView(EmptyView()))
  private var sent: [PanelEvent] = []
  private let reduceMotion: Bool
  private var frameReport: LayoutFrameReport?
  private(set) var frames: [String: CGRect] = [:]

  init(reduceMotion: Bool) {
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: PanelLayout.picture.width, height: 100),
      styleMask: .titled,
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.contentView = hosting
    self.reduceMotion = reduceMotion
    frameReport = LayoutFrameReport { [weak self] id, frame in self?.frames[id] = frame }
  }

  func show(_ controls: PanelControls) {
    hosting.rootView = AnyView(
      PanelControlRow(controls: controls) { [weak self] in self?.sent.append($0) }
        .keyboardShortcut(.defaultAction)
        .environment(\.motionReductionOverride, reduceMotion)
        .environment(\.layoutFrameReport, frameReport)
        .frame(width: PanelLayout.picture.width, height: 100)
    )
    render()
  }

  func render() {
    hosting.layoutSubtreeIfNeeded()
    hosting._renderForTest(interval: 1.0 / 60)
    hosting.layoutSubtreeIfNeeded()
    if let bitmap = hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds) {
      hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
    }
  }

  func press() -> [PanelEvent] {
    sent = []
    if let event = NSEvent.keyEvent(
      with: .keyDown,
      location: .zero,
      modifierFlags: [],
      timestamp: 0,
      windowNumber: window.windowNumber,
      context: nil,
      characters: "\r",
      charactersIgnoringModifiers: "\r",
      isARepeat: false,
      keyCode: 36
    ) {
      _ = window.performKeyEquivalent(with: event)
    }
    return sent
  }
}
