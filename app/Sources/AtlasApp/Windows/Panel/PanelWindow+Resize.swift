import AppKit
import AtlasUI

extension PanelWindow {
  /// Follows the height the panel's view says it takes, on the next turn: the view says it while it lays out, and a
  /// window must not change its size inside a layout.
  ///
  /// - Parameter height: The height the panel's card takes.
  func fit(_ height: CGFloat) {
    // Unstructured, as the view's report is synchronous.
    Task { [weak self] in self?.land(height) }
  }

  /// Gives the window the size its panel asks for, hanging from the same top edge: easing on ``PanelResize`` while it
  /// shows, so the height follows Recent and the line as they come and go, and at once while it is closed or under
  /// Reduce Motion.
  ///
  /// - Parameter height: The height the panel's card takes.
  func land(_ height: CGFloat) {
    let size = PanelWindowFrame.size(card: height)
    guard height > 0, size != frame.size else { return }
    let next = NSRect(x: frame.minX, y: frame.maxY - size.height, width: size.width, height: size.height)
    switch PanelResize.change(isShown: isOpen, reduceMotion: Appearance.reduceMotion) {
    case .instant:
      setFrame(next, display: true)
    case .eased(let seconds):
      NSAnimationContext.runAnimationGroup { context in
        context.duration = seconds
        context.timingFunction = CAMediaTimingFunction(name: .easeOut)
        animator()
          .setFrame(next, display: true)
      }
    }
  }
}
