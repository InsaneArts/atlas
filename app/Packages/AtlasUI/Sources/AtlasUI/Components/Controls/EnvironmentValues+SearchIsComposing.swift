import SwiftUI

extension EnvironmentValues {
  /// Reads the owning window's live input-method state before search handles a key.
  @Entry public var searchIsComposing: @MainActor () -> Bool = { false }
}
