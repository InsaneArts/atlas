import AppKit
import SwiftUI

@testable import AtlasUI

/// An offscreen Privacy page, drawn at a page's width, reporting the home row's and its button's frames;
/// nothing reaches the desktop.
@MainActor
final class FilesSectionHost {
  /// The window it draws in, never shown.
  private let window: NSWindow
  /// The page's host.
  private let hosting = NSHostingView(rootView: AnyView(EmptyView()))
  /// Receives each reported frame.
  private var frameReport: LayoutFrameReport?
  /// The frames reported last, by element.
  private(set) var frames: [String: CGRect] = [:]

  /// Creates the host.
  init() {
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: SettingsLayout.width, height: ControlHeight.xl * 6),
      styleMask: .titled,
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.contentView = hosting
    frameReport = LayoutFrameReport { [weak self] id, frame in self?.frames[id] = frame }
  }

  /// Shows the page in `state`.
  ///
  /// - Parameter state: The page.
  func show(_ state: SettingsFilesState) {
    hosting.rootView = AnyView(
      PrivacyPage(state: state) { _ in }
        .environment(\.layoutFrameReport, frameReport)
        .frame(width: SettingsLayout.pageWidth)
    )
    render()
  }

  /// Draws one frame, a sixtieth of a second on.
  func render() {
    hosting.layoutSubtreeIfNeeded()
    hosting._renderForTest(interval: 1.0 / 60)
    hosting.layoutSubtreeIfNeeded()
    if let bitmap = hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds) {
      hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
    }
  }
}
