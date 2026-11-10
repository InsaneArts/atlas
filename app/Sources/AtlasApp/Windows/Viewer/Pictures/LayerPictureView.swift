import AppKit
import AtlasUI
import SwiftUI

/// The live picture drawn through a layer, the app's view for ``ViewerView``'s picture slot: a layer-hosting
/// view whose layer (``LayerPicture``) takes each new picture whole. The window's state says how the picture is
/// drawn; the newest picture itself comes from the feed, which only this view observes, so a frame updates this view
/// and nothing else of the window. It never rebuilds the view or crossfades frames.
struct LayerPictureView: NSViewRepresentable {
  /// What the window's state gives the slot: the picture it is laid out with, its colour and its corners.
  let live: ViewerLivePicture
  /// The newest picture, drawn in place of the state's.
  let feed: LivePictureFeed

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
    context.coordinator.show(feed.drawn(live))
    return view
  }

  /// Shows the newest picture. The feed is read here, so SwiftUI calls this again with each new picture.
  ///
  /// - Parameters:
  ///   - view: The view.
  ///   - context: The coordinator.
  func updateNSView(_ view: NSView, context: Context) {
    context.coordinator.show(feed.drawn(live))
  }
}
