import Foundation

/// Settings' choices that outlast the window: the pane last shown, the account note the person read, and whether Atlas
/// notifies the person when it needs them, which ``AttentionSignals`` reads as it applies it. Handed to the controller, so
/// tests keep them in memory and never write the real defaults.
struct SettingsPreferences {
  /// The defaults key of the pane last shown, by its raw value. Shipped, so it never changes.
  static let tabKey = "settings.tab"
  /// The defaults key of Notify me. Shipped, so it never changes.
  static let notifyKey = "settings.notify"
  /// The defaults key of the account note the person read. Shipped, so it never changes.
  static let accountNoteKey = "settings.accountNoteRead"
  /// The defaults key of whether macOS was asked to let Atlas notify the person of its questions. Shipped, so it
  /// never changes.
  static let askConsentKey = "asks.consentAsked"

  /// The pane last shown, by its raw value; nil when none is saved.
  var loadTab: @MainActor () -> String?
  /// Saves the pane shown, by its raw value.
  var saveTab: @MainActor (String) -> Void
  /// Whether Notify me is on; off when nothing is saved, so notifications wait until the person asks for them.
  var loadNotify: @MainActor () -> Bool
  /// Saves Notify me.
  var saveNotify: @MainActor (Bool) -> Void
  /// When the newest account Atlas added after a password was, as the person read its note, in milliseconds since
  /// 1970; 0 when no note was read.
  var loadAccountNoteRead: @MainActor () -> Double = { 0 }
  /// Saves when the account whose note the person read was added.
  var saveAccountNoteRead: @MainActor (Double) -> Void = { _ in }
  /// Whether macOS was asked, once, to let Atlas notify the person of its questions.
  var loadAskConsentAsked: @MainActor () -> Bool = { false }
  /// Saves that macOS was asked.
  var saveAskConsentAsked: @MainActor () -> Void = {}

  /// Preferences kept in `defaults` under the shipped keys.
  ///
  /// - Parameter defaults: Where to keep them.
  /// - Returns: The preferences.
  static func stored(in defaults: UserDefaults) -> SettingsPreferences {
    SettingsPreferences(
      loadTab: { defaults.string(forKey: tabKey) },
      saveTab: { defaults.set($0, forKey: tabKey) },
      loadNotify: { defaults.bool(forKey: notifyKey) },
      saveNotify: { defaults.set($0, forKey: notifyKey) },
      loadAccountNoteRead: { defaults.double(forKey: accountNoteKey) },
      saveAccountNoteRead: { defaults.set($0, forKey: accountNoteKey) },
      loadAskConsentAsked: { defaults.bool(forKey: askConsentKey) },
      saveAskConsentAsked: { defaults.set(true, forKey: askConsentKey) }
    )
  }
}
