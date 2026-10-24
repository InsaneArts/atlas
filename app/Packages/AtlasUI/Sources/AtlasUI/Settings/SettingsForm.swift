import SwiftUI

/// A form pane at its insets, scrolling when its notes make it taller than the window.
struct SettingsForm<Content: View>: View {
  /// The pane.
  @ViewBuilder let content: Content

  var body: some View {
    ScrollView {
      content
        .padding(SettingsLayout.paneInsets)
    }
    .scrollIndicators(.automatic)
  }
}

#if DEBUG
  #Preview("Helper not ready") {
    SettingsForm {
      HelperPane(state: PreviewStates.settingsHelperNotReady.helper) { _ in }
    }
    .frame(width: SettingsLayout.window.width, height: SettingsLayout.window.height)
    .windowSurface()
  }
#endif
