import AppKit
import SwiftUI

/// The permission panel's draggable Atlas icon, placed by SwiftUI: an ``AppDragSource``, where the drag ghost starts.
struct AppDragIcon: NSViewRepresentable {
  /// The app dragged.
  let app: URL
  /// The tooltip.
  let help: String
  /// The drag ghost, which starts at the icon and steps aside while the person points at it or drags it.
  let ghost: DragGhost

  /// The drag source, drawing Atlas's icon.
  ///
  /// - Parameter context: SwiftUI's context.
  /// - Returns: The view.
  func makeNSView(context: Context) -> AppDragSource {
    let view = AppDragSource(app: app, image: AppIconImage.icon, label: OnboardingWords.atlas, help: help)
    view.busy = { [weak ghost] busy in ghost?.pause(busy) }
    ghost.source = view
    return view
  }

  /// Nothing changes once made.
  ///
  /// - Parameters:
  ///   - view: The view.
  ///   - context: SwiftUI's context.
  func updateNSView(_ view: AppDragSource, context: Context) {}
}
