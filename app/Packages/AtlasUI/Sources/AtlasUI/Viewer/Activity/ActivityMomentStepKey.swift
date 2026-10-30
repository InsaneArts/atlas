import SwiftUI

/// Identifies the moment whose descendant button currently holds keyboard focus.
struct ActivityMomentStepKey: FocusedValueKey {
  /// The owning moment's identity.
  typealias Value = String
}
