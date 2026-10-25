import SwiftUI

/// A native button that follows SwiftUI's enabled environment, as a standard control does.
struct SettingsNativeButton: NSViewRepresentable {
  /// The control retained by the interaction test.
  let button: SettingsButtonProbe

  /// Supplies the real native control.
  ///
  /// - Parameter context: SwiftUI's environment.
  /// - Returns: The action receiver.
  func makeNSView(context: Context) -> SettingsButtonProbe { button }

  /// Applies input ownership before an opacity animation can finish.
  ///
  /// - Parameters:
  ///   - view: The action receiver.
  ///   - context: SwiftUI's current enabled environment.
  func updateNSView(_ view: SettingsButtonProbe, context: Context) {
    view.isEnabled = context.environment.isEnabled
  }
}
