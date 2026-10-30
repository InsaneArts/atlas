#if DEBUG
  import SwiftUI

  extension EnvironmentValues {
    /// A keyboard test or interactive preview's initial session focus; nil leaves normal system focus unchanged.
    @Entry var initialSessionFocus: ViewerSessionChoice? = nil
  }
#endif
