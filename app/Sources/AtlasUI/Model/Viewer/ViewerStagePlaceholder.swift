/// Why the stage shows no picture, said as an onboarding screen says it.
package enum ViewerStagePlaceholder: Equatable, Sendable {
  /// The helper is logged out.
  ///
  /// - Parameters:
  ///   - text: What logging in involves.
  ///   - canLogIn: Whether Log In… shows: the app can lock the screen for the helper.
  case loggedOut(text: String, canLogIn: Bool)
  /// The helper's desktop cannot be shown.
  ///
  /// - Parameter reason: Why, in the app's words.
  case unavailable(reason: String)
  /// The picture is paused before one arrived.
  case paused
  /// The first picture is on its way.
  case connecting
}
