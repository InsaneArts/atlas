import SwiftUI

extension EnvironmentValues {
  /// Whether the page the pieces below belong to has arrived, as its ``PageArrivalScope`` holds it; nil with no
  /// scope above, where each piece holds its own arrival. Held above a layout that draws its pieces in more than one
  /// place, as ``ScrollColumn`` does, so a piece shown in a new place on the same page is shown at once, never faded in
  /// again.
  @Entry var pageArrived: Bool?
}
