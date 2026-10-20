#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The viewer window's design renders. Debug builds only, since the samples are.
  @MainActor
  enum ViewerRender {
    /// Every viewer state drawn for checking against the design, by file name: one for each sample the previews list
    /// (``PreviewStates/viewer``), the later ones numbered after the first nine so those keep their names.
    static let renders: [(file: String, state: ViewerState)] = [
      ("01-watching", PreviewStates.viewerWatching),
      ("02-sidebar", PreviewStates.viewerSidebar),
      ("03-sidebar-control", PreviewStates.viewerSidebarControl),
      ("04-control", PreviewStates.viewerControl),
      ("05-recovery", PreviewStates.viewerRecovery),
      ("06-paused", PreviewStates.viewerPaused),
      ("07-unavailable", PreviewStates.viewerUnavailable),
      ("08-notice", PreviewStates.viewerNotice),
      ("09-logged-out", PreviewStates.viewerLoggedOut),
      ("10-paused-before-picture", PreviewStates.viewerPausedBeforePicture),
      ("11-connecting", PreviewStates.viewerConnecting),
      ("12-typing", PreviewStates.viewerTyping),
    ]

    /// Draws every viewer state to PNGs in `directory`, at Retina scale (``PNG/scale``).
    ///
    /// - Parameters:
    ///   - directory: Where the PNGs go.
    ///   - size: The window's size.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String, size: NSSize = PreviewStates.viewerSize) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      return renders.map { render in
        PNG.write(
          ViewerView(state: render.state) { _ in },
          size: size,
          to: folder.appendingPathComponent("\(render.file).png")
        )
      }
      .allSatisfy { $0 }
    }
  }
#endif
