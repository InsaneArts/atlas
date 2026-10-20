import AtlasKit
import Foundation

/// Input from the desktop view: pointer, scrolling, keys and text, each sent only while input may go.
extension ViewerController {
  /// Scroll events are gathered this long, then sent as whole lines: 70 ms, short enough to feel immediate and long
  /// enough to gather a trackpad's burst into one batch.
  static let scrollGathering: Duration = .milliseconds(70)
  /// The pause between the move to where scrolling happens and the scroll: 80 ms, so the app under the pointer sees
  /// the pointer arrive before the wheel turns.
  static let scrollSettle = 80

  /// The display point under `location` in a view of `size`.
  ///
  /// - Parameters:
  ///   - location: The point in the view, from its top left.
  ///   - size: The view's size.
  ///   - clamp: Whether a point outside the picture moves to its nearest edge.
  /// - Returns: The point on the helper's display, or nil when there is none.
  func point(_ location: CGPoint, in size: CGSize, clamp: Bool = false) -> DesktopPoint? {
    guard let frame = reading.frame, let pictureSize = reading.pictureSize else { return nil }
    return DesktopPoint(location: location, in: size, image: pictureSize, display: frame.display, clamp: clamp)
  }

  /// View points per point of the helper's display in a view of `size`; 1 with no picture to measure by.
  ///
  /// - Parameter size: The view's size.
  /// - Returns: The view's scale.
  func scale(in size: CGSize) -> Double {
    guard let frame = reading.frame, let pictureSize = reading.pictureSize else { return 1 }
    return DesktopPoint.scale(in: size, image: pictureSize, display: frame.display) ?? 1
  }

  /// A button went down over the desktop.
  ///
  /// - Parameters:
  ///   - button: The button.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  ///   - modifiers: The modifier keys held.
  func pointerDown(_ button: MouseButton, at location: CGPoint, in size: CGSize, modifiers: Modifiers) {
    guard canInput, let point = point(location, in: size),
      let action = gesture.start(
        button,
        at: point,
        scale: scale(in: size),
        modifiers: modifiers,
        now: clock(),
        repeatWithin: doubleClickInterval()
      )
    else { return }
    enqueue([action])
  }

  /// A held button moved.
  ///
  /// - Parameters:
  ///   - button: The button.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  ///   - modifiers: The modifier keys held.
  func pointerDragged(_ button: MouseButton, to location: CGPoint, in size: CGSize, modifiers: Modifiers) {
    guard canInput, let point = point(location, in: size, clamp: true),
      let action = gesture.move(button, to: point, scale: scale(in: size), modifiers: modifiers)
    else { return }
    enqueue([action])
  }

  /// The button comes up where it is, or where the gesture last was. The release is cleanup, so it goes whenever
  /// control is held, over a stale picture too: dropped, it would leave the helper's button down, and later moves
  /// would drag.
  ///
  /// - Parameters:
  ///   - button: The button.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  ///   - modifiers: The modifier keys held.
  func pointerUp(_ button: MouseButton, at location: CGPoint, in size: CGSize, modifiers: Modifiers) {
    guard let point = point(location, in: size, clamp: true) ?? gesture.last else { return }
    let movement = gesture.move(button, to: point, scale: scale(in: size), modifiers: modifiers)
    guard let action = gesture.finish(button, at: point, modifiers: modifiers, now: clock()) else { return }
    enqueueCleanup([movement, action].compactMap { $0 })
  }

  /// The pointer moved with no button held.
  ///
  /// - Parameters:
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  func pointerMoved(to location: CGPoint, in size: CGSize) {
    guard canInput, !gesture.isActive, let point = point(location, in: size) else { return }
    enqueue([.move(x: point.x, y: point.y)])
  }

  /// Scrolling is gathered for a moment and sent as whole lines. A new gesture, one the trackpad says begins or one
  /// after a pause longer than the gathering, drops the fraction of a line the last one left.
  ///
  /// - Parameters:
  ///   - dx: AppKit's horizontal delta.
  ///   - dy: AppKit's vertical delta.
  ///   - precise: Whether it came from a trackpad, in points, rather than a wheel's lines.
  ///   - began: Whether the trackpad says this event begins a gesture.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  func scrolled(dx: Double, dy: Double, precise: Bool, began: Bool, at location: CGPoint, in size: CGSize) {
    guard canInput, let point = point(location, in: size) else { return }
    let now = clock()
    let quiet = wheelHeardAt.map { Duration.seconds(now - $0) > Self.scrollGathering } ?? true
    if began || quiet { wheel.beginGesture() }
    wheelHeardAt = now
    wheel.add(dx: dx, dy: dy, precise: precise)
    wheelAt = point
    sendWheelSoon()
  }

  /// Sends the gathered scrolling once the gathering ends, unless a send already waits.
  private func sendWheelSoon() {
    guard wheelSending == nil else { return }
    wheelSending = Task { [weak self] in
      try? await Task.sleep(for: Self.scrollGathering)
      guard let self, !Task.isCancelled else { return }
      wheelSending = nil
      sendWheel()
    }
  }

  /// Sends the whole lines gathered; lines past one send's most go with the next, which is not left waiting for
  /// another scroll event.
  private func sendWheel() {
    let delta = wheel.take()
    guard let at = wheelAt, delta.dx != 0 || delta.dy != 0 else { return }
    // AppKit's positive delta moves content down (towards the top of
    // the page); Atlas's positive lines scroll down the page.
    enqueue([
      .move(x: at.x, y: at.y), .wait(milliseconds: Self.scrollSettle),
      .scroll(x: at.x, y: at.y, dx: -delta.dx, dy: -delta.dy),
    ])
    if wheel.holdsWholeLines { sendWheelSoon() }
  }

  /// A key press: release, paste, a key, or nil when it is text for the
  /// input method. Returns whether it was handled.
  ///
  /// - Parameters:
  ///   - keyCode: The key's virtual key code.
  ///   - modifiers: The modifier keys held.
  ///   - clipboard: This Mac's clipboard text, read only for Command-V.
  /// - Returns: Whether it was handled here.
  func keyPressed(keyCode: UInt16, modifiers: Modifiers, clipboard: () -> String?) -> Bool {
    guard canInput else { return false }
    if KeyMap.isRelease(keyCode: keyCode, modifiers: modifiers) {
      releaseControl()
      return true
    }
    if KeyMap.isPaste(keyCode: keyCode, modifiers: modifiers) {
      send(text: clipboard() ?? "")
      return true
    }
    guard let action = KeyMap.action(keyCode: keyCode, modifiers: modifiers) else { return false }
    enqueue([action])
    return true
  }

  /// A key from the footer's Keys menu (Escape, a shortcut).
  ///
  /// - Parameter key: The key, as the service names it.
  func sendKey(_ key: String) {
    enqueue([.key(key)])
  }

  /// Committed text, typed in order, in chunks the helper accepts; text it would refuse is not sent, and the notice
  /// says why.
  ///
  /// - Parameter text: The text.
  func send(text: String) {
    guard !text.isEmpty, canInput else { return }
    switch TypedText(text) {
    case .chunks(let chunks):
      for chunk in chunks { enqueue([.type(text: chunk)]) }
    case .tooLong:
      say(ViewerWords.pasteTooLong(TypedText.pasteLimit))
    case .sequenceTooLong:
      say(ViewerWords.sequenceTooLong)
    }
  }
}
