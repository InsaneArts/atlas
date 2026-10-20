import AtlasUI
import CoreGraphics

/// Where the setup window sits: a card in the middle of the screen, or a narrow panel at its left, where System
/// Settings leaves room, while the person turns a switch on there. Pure geometry.
enum OnboardingPlacement {
  /// 40: the side panel's gap from the screen's left edge.
  static let sideGap: CGFloat = 40

  /// The window's content size.
  ///
  /// - Parameter guiding: Whether the guide panel shows.
  /// - Returns: The side panel's size while guiding, else the card's.
  static func size(guiding: Bool) -> CGSize {
    guiding ? OnboardingLayout.side : OnboardingLayout.card
  }

  /// The placing the window remembers after it is asked for `guiding`: a placing is done only when it differs from
  /// the last one done, and remembered only once it ran, so one the window could not do (hidden, with no screen) is
  /// done the next time it is asked for.
  ///
  /// - Parameters:
  ///   - guiding: Whether the guide panel shows now.
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
  ///   - guiding: Whether the guide panel shows.
  ///   - frame: The window's frame size.
  ///   - visible: The screen's visible frame.
  /// - Returns: The frame's origin: at the left and centred up and down while guiding, else centred both ways.
  static func origin(guiding: Bool, frame: CGSize, visible: CGRect) -> CGPoint {
    guiding
      ? CGPoint(x: visible.minX + sideGap, y: visible.midY - frame.height / 2)
      : CGPoint(x: visible.midX - frame.width / 2, y: visible.midY - frame.height / 2)
  }
}
