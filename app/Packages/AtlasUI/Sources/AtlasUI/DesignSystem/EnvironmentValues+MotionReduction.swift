import SwiftUI

extension EnvironmentValues {
  /// The system's Reduce Motion setting, shared by every motion and layout consumer.
  public var motionReduced: Bool {
    #if DEBUG
      if let motionReductionOverride { return motionReductionOverride }
    #endif
    return accessibilityReduceMotion
  }

  #if DEBUG
    /// A deterministic render's accessibility setting; nil follows the real system preference.
    @Entry public var motionReductionOverride: Bool? = nil
  #endif
}
