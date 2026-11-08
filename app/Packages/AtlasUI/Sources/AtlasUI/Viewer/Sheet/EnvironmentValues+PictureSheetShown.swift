import SwiftUI

extension EnvironmentValues {
  /// Whether the picture sheet shows over the window, so the picture or the step that opened it takes the keyboard
  /// and VoiceOver back as it closes.
  @Entry var pictureSheetShown = false
}
