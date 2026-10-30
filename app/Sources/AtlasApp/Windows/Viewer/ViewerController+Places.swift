import AtlasUI

/// The places: the Screen and the Library's lists. Leaving the Screen gives control back, as losing focus does, and Take
/// Control then waits until the release is answered; each place reads while it shows, and a screenshot shown large
/// closes with its list.
extension ViewerController {
  /// A Library row chosen: its list, or the Screen again when that list already shows, so the row leads back the way
  /// it came.
  ///
  /// - Parameter place: The row's list.
  func chooseLibrary(_ place: ViewerPlace) {
    show(place == session.place ? .screen : place)
  }

  /// Shows `place`, which reads from then on, and stops reading the one left. Control acts on the picture, so leaving
  /// the Screen gives it back.
  ///
  /// - Parameter place: The place.
  func show(_ place: ViewerPlace) {
    guard place != session.place else { return }
    if holdingControl || control.acquiring { control.releaseControl() }
    if session.place == .screenshots { panesModel.closeShot() }
    session.place = place
    noteActivitySeen()
    panesModel.followPane()
  }

  /// What the person has seen of the helper's activity. While the Activity pane shows, everything it shows is
  /// seen: its own log once read, which reads on while the picture is paused, else the status's actions. At
  /// the first reading the status's actions are seen, so what the helper did before the window opened is
  /// never new. Otherwise an unseen action that failed is remembered until Activity shows, even once newer
  /// ones push it out of the status.
  func noteActivitySeen() {
    guard reading.hasStatus else { return }
    let status = reading.statusReading.activity(limit: .max)
    var seen: ViewerSeenActivity
    if let shown = session.activitySeen, session.place != .activity {
      seen = shown
      let unseen = shown.unseen(in: status)
      seen.failed = shown.failed || unseen.contains(where: \.failed)
    } else {
      let log = session.place == .activity ? panesModel.panes.activity.log?.entries.map(\.id) : nil
      seen = ViewerSeenActivity(ids: Set(log ?? status.map(\.id)))
    }
    if session.activitySeen != seen { session.activitySeen = seen }
  }
}
