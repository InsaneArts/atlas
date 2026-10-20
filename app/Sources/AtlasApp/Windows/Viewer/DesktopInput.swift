import AppKit
import SwiftUI

/// The desktop canvas in SwiftUI: the input surface the shell passes into ``ViewerView``.
struct DesktopInput: NSViewRepresentable {
  /// The window's controller, which every input goes to.
  let controller: ViewerController

  /// Makes the canvas, named for VoiceOver.
  ///
  /// - Parameter context: SwiftUI's context.
  /// - Returns: The canvas.
  func makeNSView(context: Context) -> DesktopCanvasView {
    let view = DesktopCanvasView()
    view.controller = controller
    view.setAccessibilityElement(true)
    view.setAccessibilityRole(.group)
    view.setAccessibilityLabel("Helper desktop input")
    return view
  }

  /// The canvas leaves: its key monitor goes with it, and so does a composition under way, whose chip would
  /// otherwise stay on the stage. SwiftUI is updating its views now, so the controller drops the chip once that
  /// update has finished rather than during it.
  ///
  /// - Parameters:
  ///   - view: The canvas.
  ///   - coordinator: None.
  static func dismantleNSView(_ view: DesktopCanvasView, coordinator: ()) {
    view.stopIntercepting()
    guard view.hasMarkedText() else { return }
    view.discardComposition()
    Task { @MainActor [weak controller = view.controller] in controller?.compose("") }
  }

  /// Follows control once each update has finished.
  ///
  /// - Parameters:
  ///   - view: The canvas.
  ///   - context: SwiftUI's context.
  func updateNSView(_ view: DesktopCanvasView, context: Context) {
    view.controller = controller
    // Read here so SwiftUI calls this again when control changes.
    _ = controller.holdingControl
    // Changing the first responder during a SwiftUI update is not
    // allowed: follow control once this update has finished.
    Task { @MainActor in view.follow() }
  }
}
