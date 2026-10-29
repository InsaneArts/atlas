import SwiftUI

extension EnvironmentValues {
  /// Where the selected row of the ``CardScroll`` a view is in reports where it stands; nil outside one.
  @Entry var cardScrollSelection: CardScrollSelection?
}
