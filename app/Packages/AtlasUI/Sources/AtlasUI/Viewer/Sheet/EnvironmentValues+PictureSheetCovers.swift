import SwiftUI

extension EnvironmentValues {
  /// Whether the picture sheet stands over the window, which it does from the moment it is asked for until its closing
  /// has finished, so what opened it takes the keyboard and VoiceOver back only once it has gone.
  @Entry var pictureSheetCovers = false
}
