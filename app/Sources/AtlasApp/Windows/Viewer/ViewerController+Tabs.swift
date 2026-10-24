import AtlasUI

/// The tabs and Details: leaving the Screen tab gives control back, as losing focus does, and Take Control then waits
/// until the release is answered; each tab reads while it shows, and a screenshot shown large closes with its tab.
extension ViewerController {
  /// Shows `tab`, which reads from then on, and stops reading the one left. Control acts on the picture, so leaving
  /// the Screen tab gives it back.
  ///
  /// - Parameter tab: The tab.
  func selectTab(_ tab: ViewerTab) {
    guard tab != session.tab else { return }
    if holdingControl || control.acquiring { control.releaseControl() }
    if session.tab == .screenshots { panesModel.closeShot() }
    session.tab = tab
    noteActivitySeen()
    panesModel.followPane()
  }

  /// What the person has seen of the helper's activity (v3 V2). While the Activity tab shows, everything it shows is
  /// seen: its own log once read, which reads on while the picture is paused, else the status's actions. At the first
  /// reading the status's actions are seen, so what the helper did before the window opened is never new. Otherwise
  /// an unseen action that failed is remembered until Activity shows, even once newer ones push it out of the status.
  func noteActivitySeen() {
    guard reading.hasStatus else { return }
    let status = reading.statusReading.activity(limit: .max)
    var seen: ViewerSeenActivity
    if let shown = session.activitySeen, session.tab != .activity {
      seen = shown
      let unseen = shown.unseen(in: status)
      seen.failed = shown.failed || unseen.contains { !$0.ok }
    } else {
      let log = session.tab == .activity ? panesModel.panes.activity.log?.entries.map(\.id) : nil
      seen = ViewerSeenActivity(ids: Set(log ?? status.map(\.id)))
    }
    if session.activitySeen != seen { session.activitySeen = seen }
  }

  /// Check Again in Details: reads the status and Details' own facts now, then the next picture at once; once at a
  /// time, its button showing a spinner meanwhile.
  func checkAgain() async {
    guard !session.checkingAgain else { return }
    session.checkingAgain = true
    defer { session.checkingAgain = false }
    await readingLoop.readStatus()
    readingLoop.refresh()
    await readDetails()
  }
}
