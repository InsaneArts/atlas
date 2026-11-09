import AtlasUI

/// The places: the Screen and Activity. Leaving the Screen gives control back, as losing focus does, and Take Control
/// then waits until the release is answered; Activity reads while it shows.
extension ViewerController {
  /// Shows `place`, which reads from then on, and stops reading the one left. Control acts on the picture, so leaving
  /// the Screen gives it back.
  ///
  /// - Parameter place: The place.
  func show(_ place: ViewerPlace) {
    guard place != session.place else { return }
    // A held sign-in only stops sending input away from the Screen, and sends again back on it.
    if holdingControl || control.acquiring { control.suspend() }
    session.place = place
    control.resumeIfEligible()
    panesModel.followPane()
  }
}
