/// One row of a VM's page: its title over what it means, and at its end a value, such as "28 GB used of 128 GB", or a
/// small button.
public struct SettingsVMRow: Equatable, Sendable, Identifiable {
  /// What the row is about, unique on the page, such as `power`.
  public var id: String
  /// Its title, such as "Running".
  public var title: String
  /// What it means, under the title; nil for none.
  public var detail: String?
  /// A value at its end; nil for none.
  public var value: String?
  /// A button at its end; nil for none.
  public var button: SettingsVMButton?

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - id: What the row is about.
  ///   - title: Its title.
  ///   - detail: What it means; none by default.
  ///   - value: A value at its end; none by default.
  ///   - button: A button at its end; none by default.
  public init(
    id: String,
    title: String,
    detail: String? = nil,
    value: String? = nil,
    button: SettingsVMButton? = nil
  ) {
    self.id = id
    self.title = title
    self.detail = detail
    self.value = value
    self.button = button
  }
}
