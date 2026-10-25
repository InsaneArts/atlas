import AppKit

extension PanelPopover {
  /// Gives the popover the size its panel asks for easing on ``PanelResize`` while it shows, so the height
  /// follows Recent and the line as they come and go, and at once while it is closed or under Reduce Motion.
  ///
  /// - Parameter size: The size the panel asks for.
  func resize(to size: NSSize) {
    guard size != .zero, size != popover.contentSize else { return }
    switch PanelResize.change(isShown: popover.isShown, reduceMotion: Appearance.reduceMotion) {
    case .instant:
      // A popover animates a change of size by itself while it shows; this one must land at once.
      let animates = popover.animates
      popover.animates = false
      popover.contentSize = size
      popover.animates = animates
    case .eased(let seconds):
      NSAnimationContext.runAnimationGroup { context in
        context.duration = seconds
        context.timingFunction = CAMediaTimingFunction(name: .easeOut)
        context.allowsImplicitAnimation = true
        popover.contentSize = size
      }
    }
  }
}
