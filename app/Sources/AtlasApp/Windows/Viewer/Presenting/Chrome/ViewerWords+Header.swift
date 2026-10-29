/// The card's header's words: who has the helper and what they do with it, or when the paused picture is from.
extension ViewerWords {
  /// Who holds control while this window does.
  static let you = "You"
  /// What the person does while this window holds control.
  static let haveControl = "have control of the helper"
  /// What the session that has the helper does with it.
  static let hasTheHelper = "has the helper"
  /// The header while the picture is paused.
  static let picturePaused = "Picture paused"
  /// What goes on while the picture is paused.
  static let workGoesOn = "Work in the helper goes on"

  /// When the paused picture is from, after "Paused" in the header.
  ///
  /// - Parameter time: When it was read, on the person's clock.
  /// - Returns: "picture from 10:31".
  static func pictureFrom(_ time: String) -> String {
    "picture from \(time)"
  }
}
