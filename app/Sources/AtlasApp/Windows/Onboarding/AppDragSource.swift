import AppKit
import AtlasUI

/// Atlas.app's icon in the permission panel, dragged as Finder drags the app: its file URL, so System Settings lists
/// Atlas once it lands in a Privacy list. A click on it while System Settings is in front starts the drag without
/// bringing this window forward, and the window dims while the drag runs, so the list beside it stays in view.
final class AppDragSource: NSView, NSDraggingSource {
  /// How far the mouse moves, in points, before a press becomes a drag: 3, as AppKit's own controls wait.
  static let threshold: CGFloat = 3
  /// The window's opacity while the person drags: 0.5, so it reads as stepping back.
  static let dimmedOpacity: CGFloat = 0.5

  /// The app dragged: Atlas.app, whose helper daemon the permissions are for.
  let app: URL
  /// The icon drawn, and dragged.
  let image: NSImage
  /// Where the mouse went down, until a drag starts or the button comes up.
  private var pressedAt: NSPoint?
  /// The window dimmed for the drag running now, brought back when it ends even if the icon has left it.
  private weak var dimmedWindow: NSWindow?
  /// Told whether the person points at the icon or drags it, so the drag ghost steps aside meanwhile.
  var busy: @MainActor (Bool) -> Void = { _ in }
  /// Whether the pointer is over the icon.
  private var hovering = false
  /// Whether a drag runs.
  private var dragging = false

  /// Creates the icon.
  ///
  /// - Parameters:
  ///   - app: The app dragged.
  ///   - image: The icon drawn, and dragged.
  ///   - label: What VoiceOver calls it.
  ///   - help: The tooltip, which VoiceOver reads too.
  init(app: URL, image: NSImage, label: String, help: String) {
    self.app = app
    self.image = image
    super.init(frame: .zero)
    toolTip = help
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel(label)
    setAccessibilityHelp(help)
  }

  /// Not made from an archive.
  ///
  /// - Parameter coder: Unused.
  required init?(coder: NSCoder) { nil }

  /// A press here drags the icon, never the window, which moves by its background.
  override var mouseDownCanMoveWindow: Bool { false }

  /// The first press on the window, System Settings in front, is already a drag.
  ///
  /// - Parameter event: The press.
  /// - Returns: True.
  override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

  /// System Settings stays in front while the person drags out of this window.
  ///
  /// - Parameter event: The press.
  /// - Returns: True.
  override func shouldDelayWindowOrdering(for event: NSEvent) -> Bool { true }

  /// The icon, filling the view.
  ///
  /// - Parameter dirtyRect: The part to draw.
  override func draw(_ dirtyRect: NSRect) {
    image.draw(in: bounds)
  }

  /// An open hand over the icon while the window is key, as over anything that drags (macOS shows cursor rects in the
  /// key window only).
  override func resetCursorRects() {
    addCursorRect(bounds, cursor: .openHand)
  }

  /// Tracks the pointer over the icon even while System Settings is in front.
  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    trackingAreas.forEach(removeTrackingArea)
    addTrackingArea(
      NSTrackingArea(
        rect: .zero,
        options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
        owner: self
      )
    )
  }

  /// The pointer came over the icon.
  ///
  /// - Parameter event: The move.
  override func mouseEntered(with event: NSEvent) {
    hovering = true
    busy(hovering || dragging)
  }

  /// The pointer left the icon.
  ///
  /// - Parameter event: The move.
  override func mouseExited(with event: NSEvent) {
    hovering = false
    busy(hovering || dragging)
  }

  /// Notes where the press began, keeping System Settings in front.
  ///
  /// - Parameter event: The press.
  override func mouseDown(with event: NSEvent) {
    NSApp.preventWindowOrdering()
    pressedAt = convert(event.locationInWindow, from: nil)
  }

  /// Starts the drag once the mouse has moved far enough from the press.
  ///
  /// - Parameter event: The move.
  override func mouseDragged(with event: NSEvent) {
    guard let pressedAt else { return }
    let at = convert(event.locationInWindow, from: nil)
    guard hypot(at.x - pressedAt.x, at.y - pressedAt.y) >= Self.threshold else { return }
    self.pressedAt = nil
    let item = NSDraggingItem(pasteboardWriter: app as NSURL)
    item.setDraggingFrame(bounds, contents: image)
    beginDraggingSession(with: [item], event: event, source: self)
      .animatesToStartingPositionsOnCancelOrFail = true
  }

  /// A press that never moved far enough drags nothing.
  ///
  /// - Parameter event: The release.
  override func mouseUp(with event: NSEvent) {
    pressedAt = nil
  }

  /// Copied into another app, as Finder copies an app into a list; never dropped inside Atlas.
  ///
  /// - Parameters:
  ///   - session: The drag.
  ///   - context: Inside or outside Atlas.
  /// - Returns: Copy outside Atlas, nothing inside.
  func draggingSession(
    _ session: NSDraggingSession,
    sourceOperationMaskFor context: NSDraggingContext
  ) -> NSDragOperation {
    context == .outsideApplication ? .copy : []
  }

  /// The drag began: the window steps back.
  ///
  /// - Parameters:
  ///   - session: The drag.
  ///   - screenPoint: Where it began.
  func draggingSession(_ session: NSDraggingSession, willBeginAt screenPoint: NSPoint) {
    dimmedWindow = window
    fade(dimmedWindow, to: Self.dimmedOpacity)
    dragging = true
    busy(true)
  }

  /// The drag ended, dropped or not: the window comes back.
  ///
  /// - Parameters:
  ///   - session: The drag.
  ///   - screenPoint: Where it ended.
  ///   - operation: What the drop did.
  func draggingSession(_ session: NSDraggingSession, endedAt screenPoint: NSPoint, operation: NSDragOperation) {
    fade(dimmedWindow, to: 1)
    dimmedWindow = nil
    dragging = false
    // No pointer moves reach the icon during a drag: where the pointer is now says whether it still points at it.
    hovering = window.map { bounds.contains(convert($0.mouseLocationOutsideOfEventStream, from: nil)) } ?? false
    busy(hovering)
  }

  /// The icon leaves its window (the panel gave way to a screen mid-drag): the window comes back at once.
  ///
  /// - Parameter newWindow: The window it moves to.
  override func viewWillMove(toWindow newWindow: NSWindow?) {
    super.viewWillMove(toWindow: newWindow)
    if let dimmedWindow, dimmedWindow !== newWindow { dimmedWindow.alphaValue = 1 }
  }

  /// The icon left its window (Back, or the panel gave way to a screen) with no pointer leaving it: the ghost is
  /// free again.
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    guard window == nil else { return }
    hovering = false
    dragging = false
    busy(false)
  }

  /// Fades `window` to `opacity`.
  ///
  /// - Parameters:
  ///   - window: The window; nothing happens without one.
  ///   - opacity: Its opacity after.
  private func fade(_ window: NSWindow?, to opacity: CGFloat) {
    guard let window else { return }
    NSAnimationContext.runAnimationGroup { context in
      context.duration = Motion.Duration.fade.seconds
      window
        .animator()
        .alphaValue = opacity
    }
  }
}
