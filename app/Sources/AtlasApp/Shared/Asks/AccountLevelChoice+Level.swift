import AtlasService
import AtlasUI

extension AccountLevelChoice {
  /// The level a window shows for an account's.
  ///
  /// - Parameter level: The account's level.
  init(_ level: AccountLevel) {
    switch level {
    case .lookOnly: self = .lookOnly
    case .ask: self = .ask
    case .trusted: self = .trusted
    }
  }
}
