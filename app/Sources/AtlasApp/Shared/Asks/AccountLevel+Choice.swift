import AtlasService
import AtlasUI

extension AccountLevel {
  /// The service's level for the one the person picked in Settings.
  ///
  /// - Parameter choice: The level picked.
  init(_ choice: AccountLevelChoice) {
    switch choice {
    case .lookOnly: self = .lookOnly
    case .ask: self = .ask
    case .trusted: self = .trusted
    }
  }
}
