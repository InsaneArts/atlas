import AtlasUI

extension ViewerPresenter {
  /// The header with "Screen · Hidden from agents" at its end while a held sign-in hides the screen from agents: this
  /// window's, or one the status reports.
  ///
  /// - Parameters:
  ///   - header: The header.
  ///   - snapshot: What the controller holds.
  /// - Returns: The header, tagged while the screen is hidden.
  static func tagged(_ header: ViewerHeaderState, _ snapshot: ViewerSnapshot) -> ViewerHeaderState {
    var tagged = header
    let hidden = snapshot.session.signIn != nil || snapshot.reading.status.overview.signingIn != nil
    tagged.tag = hidden ? SignInWords.hidden : nil
    return tagged
  }
}
