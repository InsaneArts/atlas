#if DEBUG
  import SwiftUI

  extension EnvironmentValues {
    /// Real apps' icons for the app glyph's samples to stand beside, which the app's renders read from the Mac; none,
    /// as in previews, draws the stand-in icons instead.
    @Entry public var sampleAppIcons: [IconImage] = []
  }
#endif
