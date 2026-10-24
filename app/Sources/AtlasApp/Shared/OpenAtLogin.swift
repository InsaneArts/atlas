import AtlasCommands

/// Open at Login, reached through closures so tests stand in for the system's login item. Built once at launch and
/// handed to the panel, the setup window and `--login` as their `loginItem`; what it reports is `opensAtLogin`.
struct OpenAtLogin {
  /// Where Open at Login stands: whether the app can be a login item, and whether it is one.
  struct State: Equatable {
    /// Whether this app can open at login at all.
    var isAvailable: Bool
    /// Whether Atlas opens at login now.
    var isEnabled: Bool
  }

  /// Whether this app can open at login at all; a development build carries no login item.
  var isAvailable: @MainActor () -> Bool
  /// Whether Atlas opens at login now.
  var isEnabled: @MainActor () -> Bool
  /// Turns opening at login on or off; throws the system's error when it refuses.
  var set: @MainActor (Bool) throws -> Void

  /// The app's own: the LaunchAgent bundled in Atlas.app.
  ///
  /// - Parameter item: The bundled login item.
  /// - Returns: The live seam.
  static func live(_ item: LoginItem) -> OpenAtLogin {
    OpenAtLogin(
      isAvailable: { item.available },
      isEnabled: { item.enabled },
      set: { try item.set($0) }
    )
  }

  /// Where Open at Login stands now.
  @MainActor var state: State {
    State(isAvailable: isAvailable(), isEnabled: isEnabled())
  }
}
