import AppKit
import AtlasUI
import SwiftUI

@testable import AtlasUI

/// An offscreen footer button with a local Return equivalent; events never reach the desktop.
@MainActor
final class FooterButtonHost {
  private let window: NSWindow
  private let hosting = NSHostingView(rootView: AnyView(EmptyView()))
  private var sent: [ViewerEvent] = []

  init() {
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 300, height: 100),
      styleMask: .titled,
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.contentView = hosting
  }

  func show(agent: ViewerAgentButton) {
    show(FooterAgentButton(agent: agent, help: "") { [weak self] in self?.sent.append($0) })
  }

  func show(control: ViewerControlButton) {
    show(ViewerMainButton(control: control) { [weak self] in self?.sent.append($0) })
  }

  private func show(_ button: some View) {
    hosting.rootView = AnyView(button.keyboardShortcut(.defaultAction).frame(width: 300, height: 100))
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

  func press() -> [ViewerEvent] {
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
