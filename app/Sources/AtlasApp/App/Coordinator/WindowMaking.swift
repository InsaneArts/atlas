/// How the coordinator makes each window over its controller: the live AppKit windows (``live``), or a test's
/// stand-ins, which reach no screen. A window that closes by itself hands itself to `closed`, once it has closed.
struct WindowMaking {
  /// The main window.
  var viewer:
    @MainActor (ViewerController, _ closed: @escaping @MainActor (any WindowPresenting) -> Void) -> any WindowPresenting
  /// The Settings window.
  var settings:
    @MainActor (SettingsController, _ closed: @escaping @MainActor (any WindowPresenting) -> Void) ->
      any WindowPresenting
  /// The setup window.
  var onboarding:
    @MainActor (OnboardingController, _ closed: @escaping @MainActor (any WindowPresenting) -> Void) ->
      any WindowPresenting
  /// The alert window, which its controller closes.
  var alert: @MainActor (AlertController) -> any WindowPresenting

  /// The app's own windows, each on the one shell (``WindowShell``).
  static let live = WindowMaking(
    viewer: { controller, closed in ViewerWindow(controller: controller) { closed($0) } },
    settings: { controller, closed in SettingsWindow(controller: controller) { closed($0) } },
    onboarding: { controller, closed in OnboardingWindow(controller: controller) { closed($0) } },
    alert: { controller in AlertWindow(controller: controller) }
  )
}
