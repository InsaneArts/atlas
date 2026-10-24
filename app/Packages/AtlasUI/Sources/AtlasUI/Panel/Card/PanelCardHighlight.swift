import SwiftUI

/// The panel card's top highlight: the light edge along the top, fading out
/// where the corners end.
enum PanelCardHighlight {
  /// The highlight's paint, from the top edge to a tenth of the way down.
  static var gradient: LinearGradient {
    LinearGradient(
      colors: [Color.strokeHighlight, .clear],
      startPoint: .top,
      endPoint: UnitPoint(x: 0.5, y: 0.1)
    )
  }
}
