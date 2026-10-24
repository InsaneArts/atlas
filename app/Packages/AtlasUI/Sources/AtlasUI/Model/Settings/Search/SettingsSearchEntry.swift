/// Presentation metadata for one searchable location, with no action attached.
public struct SettingsSearchEntry: Equatable, Sendable, Identifiable {
  /// Stable target identity, independent of its words.
  public let id: SettingsSearchTarget
  /// The row or page's title.
  public let title: String
  /// Its brief explanation.
  public let detail: String
  /// The owning section's title.
  public let section: String
  /// The owning section's explanation.
  public let sectionDetail: String
  /// Additional local search words.
  public let aliases: [String]

  /// Creates metadata shared by presentation and search.
  ///
  /// - Parameters:
  ///   - id: The stable target.
  ///   - title: Its title.
  ///   - detail: Its explanation.
  ///   - section: Its section title.
  ///   - sectionDetail: Its section explanation.
  ///   - aliases: Useful synonyms and option names.
  public init(
    id: SettingsSearchTarget,
    title: String,
    detail: String = "",
    section: String = "",
    sectionDetail: String = "",
    aliases: [String] = []
  ) {
    self.id = id
    self.title = title
    self.detail = detail
    self.section = section
    self.sectionDetail = sectionDetail
    self.aliases = aliases
  }

  /// The owning page and section, without an invented result count.
  public var breadcrumb: String {
    let destination = id.destination
    let pages =
      destination == .root(destination.root)
      ? destination.title : "\(SettingsDestination.root(destination.root).title) › \(destination.title)"
    return section.isEmpty ? pages : "\(pages) › \(section)"
  }

  /// A brief result explanation, falling back to the section or page's presentation copy.
  public var summary: String {
    if !detail.isEmpty { return detail }
    return sectionDetail.isEmpty ? id.destination.description : sectionDetail
  }
}
