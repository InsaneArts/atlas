/// One section of a VM's page: its heading, when it has one, over its rows.
public struct SettingsVMSection: Equatable, Sendable, Identifiable {
  /// The section's heading, such as "Power and Maintenance"; nil for none.
  public var title: String?
  /// Its rows, in order.
  public var rows: [SettingsVMRow]

  /// Creates a section.
  ///
  /// - Parameters:
  ///   - title: Its heading; none by default.
  ///   - rows: Its rows, in order.
  public init(title: String? = nil, rows: [SettingsVMRow]) {
    self.title = title
    self.rows = rows
  }

  /// One section for each heading, its first row's when it has none.
  public var id: String { title ?? rows.first?.id ?? "" }
}
