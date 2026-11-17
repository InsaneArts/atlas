import AppKit

/// The standalone approval app's delegate: Quit, from the menu, ⌘Q or the Dock, goes through the window's controller,
/// so quitting refuses a request still asked and the command that asked learns at once; the app ends once the
/// refusal is kept.
final class ApprovalAppDelegate: NSObject, NSApplicationDelegate {
  /// The window's controller.
  private let controller: ApprovalController

  /// Creates the delegate.
  ///
  /// - Parameter controller: The window's controller.
  init(controller: ApprovalController) {
    self.controller = controller
  }

  /// The app is asked to end: at once when nothing is left to answer, else once the controller has closed the window.
  ///
  /// - Parameter sender: The app.
  /// - Returns: Now, or cancel while the controller closes the window, which ends the app itself.
  func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    controller.mayQuit() ? .terminateNow : .terminateCancel
  }
}
