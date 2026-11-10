import AtlasService
import AtlasUI

extension AskAnswer {
  /// The service's answer for the one the person chose in a window.
  ///
  /// - Parameter choice: The answer the person chose.
  init(_ choice: AskChoice) {
    switch choice {
    case .task: self = .task
    case .once: self = .once
    case .deny: self = .deny
    }
  }
}
