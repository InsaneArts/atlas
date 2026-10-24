import AppKit

/// The helper's two notices as the design renders draw them (v3 A7): the system alerts `--notice` shows, with the
/// words the helper's daemon gives them (`src/lib/daemon/facts.rs`), drawn offscreen through ``NoticeAlert/make``.
enum NoticeRender: String, CaseIterable, Sendable {
  /// "Atlas is set up here": the daemon names no account, so OK alone.
  case setUp
  /// "Atlas is running here again": Switch Back to the person's own account, or Stay Here.
  case runningAgain

  /// The notice's title.
  var title: String {
    switch self {
    case .setUp: "Atlas is set up here"
    case .runningAgain: "Atlas is running here again"
    }
  }

  /// What it says, a sample account's name in place of the person's.
  var message: String {
    switch self {
    case .setUp:
      "Screen Recording and Accessibility are on. Switch back to your own account; Atlas carries on there by itself."
    case .runningAgain: "Switch back to Alex; Atlas carries on there by itself."
    }
  }

  /// The account Switch Back is for; nil for OK alone.
  var switchBack: String? {
    switch self {
    case .setUp: nil
    case .runningAgain: "alex"
    }
  }

  /// Draws the notice as the system lays it out, on a window's ground, to a PNG at `path`.
  ///
  /// - Parameters:
  ///   - path: Where to write the PNG.
  ///   - appearance: The appearance to draw it in.
  /// - Returns: Whether the PNG was written.
  func draw(to path: String, appearance: NSAppearance?) -> Bool {
    let alert = NoticeAlert.make(title: title, message: message, switchBack: switchBack)
    alert.window.appearance = appearance
    alert.layout()
    guard let appearance, let frame = alert.window.contentView?.superview,
      let data = PNG.data(of: frame, appearance: appearance)
    else { return false }
    return (try? data.write(to: URL(fileURLWithPath: path))) != nil
  }
}
