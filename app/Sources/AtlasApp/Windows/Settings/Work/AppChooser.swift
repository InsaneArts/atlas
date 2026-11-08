import AppKit
import UniformTypeIdentifiers

/// The panel the person chooses an app in, to add it to the helper's accounts: only apps, one at a time, from the
/// Applications folder.
enum AppChooser {
  /// Asks the person for an app.
  ///
  /// - Returns: The app chosen, or nil when they cancel or it has no bundle identifier.
  @MainActor static func choose() async -> ChosenApp? {
    let panel = NSOpenPanel()
    panel.allowedContentTypes = [.applicationBundle]
    panel.allowsMultipleSelection = false
    panel.canChooseDirectories = false
    panel.directoryURL = URL(fileURLWithPath: "/Applications", isDirectory: true)
    panel.prompt = SettingsWords.addAccountPrompt
    panel.message = SettingsWords.addAccountMessage
    guard await panel.begin() == .OK, let url = panel.url else { return nil }
    return app(at: url)
  }

  /// The app at `url`, by its bundle's identifier and display name.
  ///
  /// - Parameter url: The app's bundle.
  /// - Returns: The app, or nil when it has no bundle identifier.
  static func app(at url: URL) -> ChosenApp? {
    guard let bundle = Bundle(url: url), let id = bundle.bundleIdentifier else { return nil }
    let shown = bundle.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
    let named = bundle.object(forInfoDictionaryKey: "CFBundleName") as? String
    return ChosenApp(bundleID: id, name: shown ?? named ?? url.deletingPathExtension().lastPathComponent)
  }
}
