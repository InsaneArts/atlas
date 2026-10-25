import SwiftUI

extension EnvironmentValues {
  /// Whether a row draws the separator above it, inset to its own text. ``GroupedList`` turns it on for every row
  /// but its first; a row on its own draws none.
  @Entry var drawsListSeparator = false
}
