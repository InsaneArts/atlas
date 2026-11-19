/// A VM's page: its card with Atlas's mark, its dot, its title and what it is, then its rows in their sections, a
/// line while one of its commands runs, what the last one said, and Add a Desktop… at its foot while it is the one
/// desktop.
public struct SettingsVMState: Equatable, Sendable {
  /// The VM's ID.
  public var id: String
  /// Its title, such as "work" or "VM".
  public var title: String
  /// What it is, with its screen's size, such as "A virtual Mac on this Mac · 1920 × 1200".
  public var line: String
  /// Its dot's tone: green while it runs, blue while the person has it open for maintenance.
  public var tone: StatusTone
  /// Its rows, in their sections, in order.
  public var sections: [SettingsVMSection]
  /// What runs now, such as "Asking in the approval window…", while its buttons wait; nil while nothing does.
  public var waiting: String?
  /// What the last command said, under the last section; nil for nothing.
  public var notice: SettingsNote?
  /// Whether Add a Desktop… shows at its foot: while it is the one desktop.
  public var addsDesktop: Bool

  /// Creates the page's state.
  ///
  /// - Parameters:
  ///   - id: The VM's ID.
  ///   - title: Its title.
  ///   - line: What it is, with its screen's size.
  ///   - tone: Its dot's tone.
  ///   - sections: Its rows, in their sections.
  ///   - waiting: What runs now; none by default.
  ///   - notice: What the last command said; none by default.
  ///   - addsDesktop: Whether Add a Desktop… shows at its foot; not by default.
  public init(
    id: String,
    title: String,
    line: String,
    tone: StatusTone,
    sections: [SettingsVMSection],
    waiting: String? = nil,
    notice: SettingsNote? = nil,
    addsDesktop: Bool = false
  ) {
    self.id = id
    self.title = title
    self.line = line
    self.tone = tone
    self.sections = sections
    self.waiting = waiting
    self.notice = notice
    self.addsDesktop = addsDesktop
  }
}
