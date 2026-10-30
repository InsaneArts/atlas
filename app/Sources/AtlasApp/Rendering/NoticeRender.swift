import AppKit

/// The helper's two notices as the design renders draw them: the system alerts `--notice` shows, with the words the
/// helper's daemon gives them, drawn offscreen through ``NoticeAlert/make``. Both name the account to switch back to,
/// so Switch Back is the default and Stay Here takes Escape.
enum NoticeRender: String, CaseIterable, Sendable {
  /// "Atlas is set up": Switch Back to the person's own account, or Stay Here.
  case setUp
  /// "Atlas is running here again": Switch Back to the person's own account, or Stay Here.
  case runningAgain

  /// The notice's title.
  var title: String {
    switch self {
    case .setUp: "Atlas is set up"
    case .runningAgain: "Atlas is running here again"
    }
  }

  /// What it says, a sample account's name in place of the person's.
  var message: String {
    switch self {
    case .setUp: "Screen Recording and Accessibility are on. Switch back to Alex — Atlas keeps running there."
    case .runningAgain: "Switch back to Alex — Atlas keeps running there."
    }
  }

  /// The account Switch Back is for.
  var switchBack: String? {
    "alex"
  }

  /// Draws the notice as the system lays it out, on a window's ground, to a PNG at `path`.
  ///
  /// - Parameters:
  ///   - path: Where to write the PNG.
  ///   - appearance: The appearance to draw it in.
  /// - Returns: Whether the PNG was written.
  func draw(to path: String, appearance: NSAppearance?) -> Bool {
    let alert = NoticeAlert.make(title: title, message: message, switchBack: switchBack, onConsole: true)
    alert.window.appearance = appearance
    alert.layout()
    guard let appearance, let frame = alert.window.contentView?.superview,
      let data = PNG.data(of: frame, appearance: appearance)
    else { return false }
    // The render reports a failed write as false; why it failed is not its concern.
    return (try? data.write(to: URL(fileURLWithPath: path))) != nil
  }
}
