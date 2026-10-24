import AtlasUI
import CoreGraphics

/// Where the setup window sits: a card in the middle of the screen, or, while the person turns a switch on in System
/// Settings, a narrow panel beside its window (at the screen's left when it shows none). Pure geometry.
enum OnboardingPlacement {
  /// 40: the side panel's gap from the screen's left edge.
  static let sideGap: CGFloat = 40
  /// 16: the side panel's gap from System Settings' window.
  static let settingsGap: CGFloat = 16
  /// 1: how far, in points, System Settings' window moves before the panel follows it.
  static let followSlack: CGFloat = 1

  /// The window's content size.
  ///
  /// - Parameter guiding: Whether the permission panel shows.
  /// - Returns: The side panel's size while guiding, else the card's.
  static func size(guiding: Bool) -> CGSize {
    guiding ? OnboardingLayout.side : OnboardingLayout.card
  }

  /// The placing the window remembers after it is asked for `guiding`: a placing is done only when it differs from
  /// the last one done, and remembered only once it ran, so one the window could not do (hidden, with no screen) is
  /// done the next time it is asked for.
  ///
  /// - Parameters:
  ///   - guiding: Whether the permission panel shows now.
  ///   - last: The last placing done; nil before the first.
  ///   - place: Moves the window, returning whether it could.
  /// - Returns: The placing to remember.
  static func placed(guiding: Bool, last: Bool?, place: (Bool) -> Bool) -> Bool? {
    guard last != guiding else { return last }
    return place(guiding) ? guiding : last
  }

  /// Where the window's frame starts.
  ///
  /// - Parameters:
  ///   - guiding: Whether the permission panel shows.
  ///   - frame: The window's frame size.
  ///   - visible: The screen's visible frame.
  /// - Returns: The frame's origin: at the left and centred up and down while guiding, else centred both ways.
  static func origin(guiding: Bool, frame: CGSize, visible: CGRect) -> CGPoint {
    guiding
      ? CGPoint(x: visible.minX + sideGap, y: visible.midY - frame.height / 2)
      : CGPoint(x: visible.midX - frame.width / 2, y: visible.midY - frame.height / 2)
  }

  /// Where the side panel's frame starts beside System Settings' window, tops aligned: at its right where the screen
  /// leaves room, else at its left, else at the screen's left edge, over System Settings' sidebar rather than its
  /// list; always within the screen.
  ///
  /// - Parameters:
  ///   - frame: The window's frame size.
  ///   - settings: System Settings' window frame.
  ///   - visible: The visible frame of the screen System Settings is on.
  /// - Returns: The frame's origin.
  static func docked(frame: CGSize, beside settings: CGRect, visible: CGRect) -> CGPoint {
    let right = settings.maxX + settingsGap
    let left = settings.minX - settingsGap - frame.width
    let x =
      if right + frame.width <= visible.maxX { right } else if left >= visible.minX { left } else { visible.minX }
    return clamped(CGPoint(x: x, y: settings.maxY - frame.height), frame: frame, visible: visible)
  }

  /// Where the side panel's frame starts while System Settings opens: shrunk around the middle of where the window
  /// was, within the screen.
  ///
  /// - Parameters:
  ///   - frame: The panel's frame size.
  ///   - current: The window's frame now.
  ///   - visible: The screen's visible frame.
  /// - Returns: The frame's origin.
  static func shrunk(frame: CGSize, from current: CGRect, visible: CGRect) -> CGPoint {
    clamped(
      CGPoint(x: current.midX - frame.width / 2, y: current.midY - frame.height / 2),
      frame: frame,
      visible: visible
    )
  }

  /// `origin`, moved as little as it takes to keep a frame `frame` big within the screen.
  ///
  /// - Parameters:
  ///   - origin: Where the frame would start.
  ///   - frame: The frame's size.
  ///   - visible: The screen's visible frame.
  /// - Returns: The origin within the screen.
  private static func clamped(_ origin: CGPoint, frame: CGSize, visible: CGRect) -> CGPoint {
    CGPoint(
      x: min(max(origin.x, visible.minX), visible.maxX - frame.width),
      y: min(max(origin.y, visible.minY), visible.maxY - frame.height)
    )
  }
}
