import SwiftUI

extension EnvironmentValues {
  /// How wide the Activity outline's card is, so a narrow one hides a run's summary, shows fewer pictures beside a
  /// link and shows Replay's symbol alone; wide until measured.
  @Entry var outlineWidth: CGFloat = .infinity
}
