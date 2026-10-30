import SwiftUI

extension EnvironmentValues {
  /// Defers a selected row's final placement out of layout; normally a main-actor task.
  @Entry var cardScrollPlacement = CardScrollPlacement()
}
