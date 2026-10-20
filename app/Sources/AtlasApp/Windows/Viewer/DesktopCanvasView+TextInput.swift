import AppKit

/// The input method's side: composed text shows until committed, then is typed.
// `@preconcurrency`: NSTextInputClient predates Swift concurrency and is not main-actor bound, but AppKit calls it on
// the main thread only, as it does every responder, so the view's main-actor methods conform as they are.
extension DesktopCanvasView: @preconcurrency NSTextInputClient {
  /// 16: how far in from the desktop's left edge the input method's candidate window opens, clear of the stage's
  /// rounded corner.
  static let candidateInset: CGFloat = 16
  /// 48: how far up from the desktop's bottom edge the candidate window's line sits, clear of the stage's rounded
  /// corner.
  static let candidateRise: CGFloat = 48
  /// 20: the height of the line the candidate window opens under, one line of text.
  static let candidateLineHeight: CGFloat = 20

  /// Committed text: the composition ends and the text is typed.
  ///
  /// - Parameters:
  ///   - string: The text, plain or attributed.
  ///   - replacementRange: The range it replaces, which the desktop ignores.
  func insertText(_ string: Any, replacementRange: NSRange) {
    let text = (string as? NSAttributedString)?.string ?? (string as? String) ?? ""
    marked = NSAttributedString()
    markedSelection = NSRange(location: 0, length: 0)
    controller?.compose("")
    controller?.send(text: text)
  }

  /// Text being composed: shown on the stage, not sent.
  ///
  /// - Parameters:
  ///   - string: The text, plain or attributed.
  ///   - selectedRange: The input method's selection within it.
  ///   - replacementRange: The range it replaces, which the desktop ignores.
  func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
    marked = (string as? NSAttributedString) ?? NSAttributedString(string: (string as? String) ?? "")
    let end = marked.length
    let location = min(max(selectedRange.location == NSNotFound ? end : selectedRange.location, 0), end)
    markedSelection = NSRange(location: location, length: min(selectedRange.length, end - location))
    controller?.compose(marked.string)
  }

  /// The composition ends, unsent.
  func unmarkText() {
    discardComposition()
    controller?.compose("")
  }

  /// The composition ends, unsent, here and in the input method, telling the controller nothing: for a canvas that
  /// leaves, whose controller hears of it afterwards.
  func discardComposition() {
    marked = NSAttributedString()
    markedSelection = NSRange(location: 0, length: 0)
    inputContext?.discardMarkedText()
  }

  /// Whether text is being composed.
  ///
  /// - Returns: Whether any is.
  func hasMarkedText() -> Bool { marked.length > 0 }

  /// The composed text's range.
  ///
  /// - Returns: All of it, or none.
  func markedRange() -> NSRange {
    hasMarkedText() ? NSRange(location: 0, length: marked.length) : NSRange(location: NSNotFound, length: 0)
  }

  /// The input method's selection within the composed text.
  ///
  /// - Returns: The selection.
  func selectedRange() -> NSRange { markedSelection }

  /// The composed text carries no attributes.
  ///
  /// - Returns: None.
  func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }

  /// The desktop holds no text of its own to offer.
  ///
  /// - Parameters:
  ///   - range: The range asked for.
  ///   - actualRange: Unchanged.
  /// - Returns: Nothing.
  func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
    nil
  }

  /// No character lies under a point.
  ///
  /// - Parameter point: The point.
  /// - Returns: Not found.
  func characterIndex(for point: NSPoint) -> Int { NSNotFound }

  /// The input method's candidate window sits at the desktop's bottom left.
  ///
  /// - Parameters:
  ///   - range: The range asked for.
  ///   - actualRange: Unchanged.
  /// - Returns: A rect in screen coordinates near the bottom left.
  func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
    let corner = NSRect(
      x: bounds.minX + Self.candidateInset,
      y: bounds.maxY - Self.candidateRise,
      width: 1,
      height: Self.candidateLineHeight
    )
    return window?.convertToScreen(convert(corner, to: nil)) ?? .zero
  }
}
