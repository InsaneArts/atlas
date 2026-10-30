import SwiftUI

/// The outline's own measures for its edge.
enum OutlineHighlightLayout {
  /// 2: the blue ring, just outside the part picked out.
  static let ringWidth = Stroke.thick
  /// 6: how far the halo reaches outside the part, the ring's 2 under its inner edge.
  static let haloWidth: CGFloat = 6
  #if DEBUG
    /// The part the preview picks out of the sample desktop: a window-sized piece near its top left.
    static let sampleFrame = CGRect(x: 120, y: 60, width: 220, height: 160)
    /// The dim the preview draws, the viewer's.
    static let sampleDim = 0.36
  #endif
}
