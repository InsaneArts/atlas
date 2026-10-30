import SwiftUI

extension FocusedValues {
  /// The moment whose step currently owns keyboard focus.
  var activityMomentStep: String? {
    get { self[ActivityMomentStepKey.self] }
    set { self[ActivityMomentStepKey.self] = newValue }
  }
}
