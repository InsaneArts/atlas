import SwiftUI

extension EnvironmentValues {
  /// Uses opacity for result marks in a surface that calls for quiet arrivals. Other surfaces retain their motion.
  @Entry var marksFade = false
}
