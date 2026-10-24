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
    panesModel.followPane()
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
