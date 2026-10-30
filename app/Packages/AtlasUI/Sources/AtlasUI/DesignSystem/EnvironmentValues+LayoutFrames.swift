#if DEBUG
  import SwiftUI

  extension EnvironmentValues {
    /// Where a debug renderer observes real layout, without relying on accessibility service timing.
    @Entry public var layoutFrameReport: LayoutFrameReport? = nil
  }
#endif
