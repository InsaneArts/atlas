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
    if holdingControl || session.acquiring { releaseControl() }
    if session.tab == .screenshots { closeShot() }
    session.tab = tab
    followPane()
  }

  /// Check Again in Details: reads the status and Details' own facts now, then the next picture at once; once at a
  /// time, its button showing a spinner meanwhile.
  func checkAgain() async {
    guard !session.checkingAgain else { return }
    session.checkingAgain = true
    defer { session.checkingAgain = false }
    await readStatus()
    refresh()
    await readDetails()
  }
}
