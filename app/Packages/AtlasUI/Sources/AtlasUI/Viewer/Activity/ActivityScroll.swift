/// Where the Activity list holds as new lines arrive at its top (v3 X10): at the top it stays at the top, so each new
/// line shows as it arrives; scrolled down, the line the person scrolled to stays where it is while lines arrive
/// above it, so what they read never moves.
enum ActivityScroll {
  /// The line the list keeps at its top, for its scroll position.
  ///
  /// - Parameters:
  ///   - line: The line the person last scrolled to the top, if any.
  ///   - isAtTop: Whether the list sits at its very top.
  /// - Returns: None at the top, so the list stays there; else the line scrolled to, which holds its place.
  static func anchor(scrolledTo line: String?, isAtTop: Bool) -> String? {
    isAtTop ? nil : line
  }
}
