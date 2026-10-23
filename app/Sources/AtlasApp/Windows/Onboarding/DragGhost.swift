import AppKit
import AtlasUI
import Observation
import SwiftUI

/// The drag ghost's window: see-through and click-through, over System Settings and the permission panel, never
/// taking focus. It shows ``DragGhostView`` from the panel's icon into System Settings' list while the panel waits
/// for its switch, and hides while the person points at the icon or drags it.
@MainActor
@Observable
final class DragGhost {
  /// Where Atlas lands in System Settings' window, as fractions of its width from the left and of its height from the
  /// top: past the sidebar, in the upper part of the list (for Screen Recording, the top one of its two lists).
  static let landing = CGPoint(x: 0.62, y: 0.38)
  /// 80: how far the overlay reaches past System Settings and the panel, so the ghost is never cut at their edges.
  static let margin: CGFloat = 80

  /// Where the ghost travels now, in the overlay's points; nil while it hides.
  private(set) var state: DragGhostState?
  /// The panel's icon, where the ghost starts.
  @ObservationIgnored weak var source: NSView?
  /// The overlay.
  @ObservationIgnored private let panel: NSPanel
  /// Whether the person points at the icon or drags it.
  @ObservationIgnored private var busy = false

  /// Creates the overlay, hidden.
  init() {
    panel = NSPanel(
      contentRect: .zero,
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: true
    )
    panel.isOpaque = false
    panel.backgroundColor = .clear
    panel.hasShadow = false
    panel.ignoresMouseEvents = true
    panel.isReleasedWhenClosed = false
    panel.hidesOnDeactivate = false
    panel.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + 1)
    panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle, .transient]
    panel.setAccessibilityElement(false)
    let host = NSHostingView(
      rootView: ShellRoot { [weak self] in
        if let state = self?.state { DragGhostView(state: state) }
      }
    )
    host.sizingOptions = []
    panel.contentView = host
  }

  /// Shows the ghost from the panel's icon into System Settings' window, or hides it.
  ///
  /// - Parameter settings: System Settings' window frame, in AppKit's coordinates; nil hides the ghost.
  func show(into settings: CGRect?) {
    guard let settings, !busy, let source, let window = source.window, window.isVisible else { return hide() }
    let icon = window.convertToScreen(source.convert(source.bounds, to: nil))
    let (area, next) = Self.path(icon: icon, settings: settings, window: window.frame)
    if panel.frame != area { panel.setFrame(area, display: false) }
    if state != next { state = next }
    if !panel.isVisible { panel.orderFront(nil) }
  }

  /// The overlay's frame, over both windows and past their edges, and the ghost's path in it: from the middle of the
  /// panel's icon to its landing in System Settings' list, measured from the overlay's top left.
  ///
  /// - Parameters:
  ///   - icon: The panel's icon, on screen.
  ///   - settings: System Settings' window frame.
  ///   - window: The setup window's frame.
  /// - Returns: The overlay's frame, and the path.
  static func path(icon: CGRect, settings: CGRect, window: CGRect) -> (area: CGRect, state: DragGhostState) {
    let target = CGPoint(
      x: settings.minX + settings.width * landing.x,
      y: settings.maxY - settings.height * landing.y
    )
    let area = settings.union(window)
      .insetBy(dx: -margin, dy: -margin)
    let local = { (point: CGPoint) in CGPoint(x: point.x - area.minX, y: area.maxY - point.y) }
    return (area, DragGhostState(from: local(CGPoint(x: icon.midX, y: icon.midY)), to: local(target)))
  }

  /// Hides the ghost.
  func hide() {
    state = nil
    if panel.isVisible { panel.orderOut(nil) }
  }

  /// The person points at the icon or drags it, or stopped: the ghost steps aside meanwhile.
  ///
  /// - Parameter busy: Whether they do now.
  func pause(_ busy: Bool) {
    self.busy = busy
    if busy { hide() }
  }

  /// Closes the overlay for good, with its window.
  func close() {
    hide()
    panel.close()
  }
}
