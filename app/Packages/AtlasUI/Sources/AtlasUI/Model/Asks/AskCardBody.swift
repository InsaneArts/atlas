import CoreGraphics

/// What a question shows of what would be sent: the message itself, a picture of the element ringed in blue, or a
/// line in place of either, such as how many characters go into a password field.
public enum AskCardBody: Equatable, Sendable {
  /// The message a Return would send, under its label.
  ///
  /// - Parameter text: The message, or its start.
  case message(String)
  /// A part of the screen before the action, cut to the well's shape (``AskCardState/pictureAspect``), with the
  /// element ringed.
  ///
  /// - Parameters:
  ///   - picture: The part of the screen.
  ///   - ring: The element, as shares of the part from its top-leading corner.
  case picture(DesktopPicture, ring: CGRect)
  /// A line in place of what would be sent, such as "12 characters in a password field".
  ///
  /// - Parameter text: The line.
  case note(String)
}
