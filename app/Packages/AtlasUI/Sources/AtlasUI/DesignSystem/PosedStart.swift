#if DEBUG
  import SwiftUI

  /// Where a posed animation (``PosedAnimation``) keeps the moment it began: the moment held when it first ran, in
  /// seconds since the change being posed; nil until it runs.
  struct PosedStart: AnimationStateKey {
    /// None until the animation first runs.
    static var defaultValue: TimeInterval? { nil }
  }
#endif
