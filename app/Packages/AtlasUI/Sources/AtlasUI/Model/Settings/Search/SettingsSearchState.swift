import Foundation

/// Transient search presentation, separate from navigation history and preferences.
public struct SettingsSearchState: Equatable, Sendable {
  /// The field's unmodified text, including input-method edits.
  public var query = ""
  /// Ranked local matches.
  public var results: [SettingsSearchEntry] = []
  /// The selected result's stable identity.
  public var selection: SettingsSearchTarget?
  /// Explicit Find requests; routine status changes leave this alone.
  public var focusRequests = 0

  /// Creates an empty search.
  public init() {}

  /// Whether the page area shows results.
  public var isActive: Bool { !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}
