import AtlasUI

/// Precomputed search words and the stable presentation-order tie break for one target.
struct SettingsSearchFields {
  /// The result to present.
  let entry: SettingsSearchEntry
  /// The normalized title used for ranking.
  let title: String
  /// All permitted search words.
  let text: String
  /// Stable page order.
  let page: Int
  /// Stable row order.
  let order: Int

  /// Precomputes a presentation entry's search fields.
  ///
  /// - Parameters:
  ///   - entry: The presentation entry.
  ///   - order: Its presentation order.
  init(entry: SettingsSearchEntry, order: Int) {
    self.entry = entry
    self.order = order
    title = SettingsSearchIndex.normalize(entry.title)
    text = SettingsSearchIndex.normalize(
      ([entry.title, entry.detail, entry.section, entry.sectionDetail] + entry.aliases).joined(separator: " ")
    )
    page =
      SettingsDestination.pages
      .firstIndex(of: entry.id.destination) ?? SettingsDestination.pages.count
  }
}
