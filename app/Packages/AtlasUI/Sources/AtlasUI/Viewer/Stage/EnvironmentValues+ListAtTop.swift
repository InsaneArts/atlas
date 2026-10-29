import SwiftUI

extension EnvironmentValues {
  /// Whether the list a view is in sits at its very top (``CardScroll``), so rows coming in above can move the rest
  /// down; scrolled down, they come with no motion and what the person reads holds its place.
  @Entry var listIsAtTop = true
}
