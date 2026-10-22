import Testing

@testable import AtlasApp

@MainActor
@Suite("AlertRouting")
struct AlertRoutingTests {
  /// Where the stand-in windows showed alerts, and whether the main window is open.
  @MainActor
  final class Shown {
    /// Whether the main window is open.
    var viewerOpen = true
    /// The alerts the main window showed.
    var inViewer: [HelperAlert] = []
    /// The alerts the alert window showed.
    var inWindow: [HelperAlert] = []
  }

  @Test("the panel's alert goes to the main window while it is open, and to the alert window otherwise")
  func theAlertGoesWhereItBelongs() {
    let shown = Shown()
    let routing = AlertRouting(
      viewerOpen: { shown.viewerOpen },
      inViewer: { shown.inViewer.append($0) },
      inWindow: { shown.inWindow.append($0) }
    )
    routing.show(.recover)
    shown.viewerOpen = false
    routing.show(.logOut)
    #expect(shown.inViewer == [.recover])
    #expect(shown.inWindow == [.logOut])
  }
}
