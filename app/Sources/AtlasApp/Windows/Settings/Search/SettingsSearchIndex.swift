import AtlasUI
import Foundation

/// A pure local index with normalized fields computed only when its metadata changes.
struct SettingsSearchIndex {
  /// The source metadata, used to avoid normalizing unchanged readings.
  let entries: [SettingsSearchEntry]
  /// Search fields in stable page and presentation order.
  private let fields: [SettingsSearchFields]

  /// Normalizes whitespace, case and diacritics independently of the user's locale.
  ///
  /// - Parameter text: The query or presentation text.
  /// - Returns: Folded words separated by one space.
  static func normalize(_ text: String) -> String {
    text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "en_US_POSIX"))
      .split(whereSeparator: \.isWhitespace)
      .joined(separator: " ")
  }

  /// Creates an index from presentation metadata, deduplicating targets while retaining their aliases.
  ///
  /// - Parameter entries: Local presentation entries in row order.
  init(entries: [SettingsSearchEntry]) {
    self.entries = entries
    var unique: [SettingsSearchEntry] = []
    var positions: [SettingsSearchTarget: Int] = [:]
    for entry in entries {
      if let position = positions[entry.id] {
        let first = unique[position]
        unique[position] = SettingsSearchEntry(
          id: first.id,
          title: first.title,
          detail: first.detail,
          section: first.section,
          sectionDetail: first.sectionDetail,
          aliases: first.aliases + [entry.title, entry.detail, entry.section, entry.sectionDetail] + entry.aliases
        )
      } else {
        positions[entry.id] = unique.count
        unique.append(entry)
      }
    }
    fields = unique.enumerated()
      .map { SettingsSearchFields(entry: $0.element, order: $0.offset) }
  }

  /// Matches every term across title, description and aliases, with deterministic title-first ranking.
  ///
  /// - Parameter query: The unmodified field text.
  /// - Returns: Ranked locations, never actions.
  func results(for query: String) -> [SettingsSearchEntry] {
    let normalized = Self.normalize(query)
    guard !normalized.isEmpty else { return [] }
    let terms = normalized.split(separator: " ")
      .map(String.init)
    return fields.compactMap { field -> (SettingsSearchFields, Int)? in
      guard terms.allSatisfy({ field.text.contains($0) }) else { return nil }
      let rank =
        field.title == normalized
        ? 0
        : field.title.hasPrefix(normalized)
          ? 1
          : terms.allSatisfy({ field.title.contains($0) }) ? 2 : 3
      return (field, rank)
    }
    .sorted {
      if $0.1 != $1.1 { return $0.1 < $1.1 }
      if $0.0.page != $1.0.page { return $0.0.page < $1.0.page }
      return $0.0.order < $1.0.order
    }
    .map { $0.0.entry }
  }
}
