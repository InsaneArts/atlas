import AppKit
import AtlasUI
import SwiftUI

/// The live picture drawn through a layer (v3 V17), the app's view for ``ViewerView``'s picture slot: a layer-hosting
/// view whose layer (``LayerPicture``) takes each new picture whole. SwiftUI only hands it the newest picture; it never
/// rebuilds the view or crossfades frames.
struct LayerPictureView: NSViewRepresentable {
  /// What the slot draws.
  let live: ViewerLivePicture

  /// The coordinator that owns the layer.
  ///
  /// - Returns: A new coordinator.
  func makeCoordinator() -> LayerPicture {
    LayerPicture()
  }

  /// A view hosting the coordinator's layer, which may take Core Image filters.
  ///
  /// - Parameter context: The coordinator.
  /// - Returns: The view.
  func makeNSView(context: Context) -> NSView {
    let view = NSView()
    view.layer = context.coordinator.layer
    view.wantsLayer = true
    view.layerUsesCoreImageFilters = true
    context.coordinator.show(live)
    return view
  }

  /// Shows the newest picture.
  ///
  /// - Parameters:
  ///   - view: The view.
  ///   - context: The coordinator.
  func updateNSView(_ view: NSView, context: Context) {
    context.coordinator.show(live)
  }
}
