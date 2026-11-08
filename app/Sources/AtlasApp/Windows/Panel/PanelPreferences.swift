import Foundation

/// The menu bar panel's one choice that outlasts it: whether the person said OK to the note that a macOS service in
/// the helper slowly slows their Mac, so it never shows again. Handed to the panel's controller, so tests keep it in
/// memory and never write the real defaults.
struct PanelPreferences {
  /// The defaults key of whether the person said OK to the service note. Shipped, so it never changes.
  static let serviceNoteKey = "panel.serviceNoteDismissed"

  /// Whether the person said OK to the service note; false when nothing is saved.
  var loadServiceNoteDismissed: @MainActor () -> Bool = { false }
  /// Saves that the person said OK to the service note.
  var saveServiceNoteDismissed: @MainActor () -> Void = {}

  /// Preferences kept in `defaults` under the shipped key.
  ///
  /// - Parameter defaults: Where to keep them.
  /// - Returns: The preferences.
  static func stored(in defaults: UserDefaults) -> PanelPreferences {
    PanelPreferences(
      loadServiceNoteDismissed: { defaults.bool(forKey: serviceNoteKey) },
      saveServiceNoteDismissed: { defaults.set(true, forKey: serviceNoteKey) }
    )
  }
}
