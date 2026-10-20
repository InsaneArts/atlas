import AtlasKit
import Foundation

@testable import AtlasApp

/// A stand-in login item: available or not, on or off, and refusing to change when told, so no test registers the
/// real one.
@MainActor
final class ScriptedLoginItem {
  /// Whether the app can open at login.
  var available: Bool
  /// Whether it opens at login now.
  var enabled: Bool
  /// Why the system refuses a change, when it does.
  var refusal: ServiceError?

  /// A login item as given.
  ///
  /// - Parameters:
  ///   - available: Whether the app can open at login.
  ///   - enabled: Whether it opens at login now.
  init(available: Bool = false, enabled: Bool = false) {
    self.available = available
    self.enabled = enabled
  }

  /// The seam the controllers take, reading and changing this item.
  var seam: OpenAtLogin {
    OpenAtLogin(
      isAvailable: { self.available },
      isEnabled: { self.enabled },
      set: { on in
        if let refusal = self.refusal { throw refusal }
        self.enabled = on
      }
    )
  }
}
