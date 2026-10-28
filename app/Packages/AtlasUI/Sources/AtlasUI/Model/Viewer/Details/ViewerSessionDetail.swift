/// The chosen host session in Details: who it is, where it runs, and what it made in the helper.
public struct ViewerSessionDetail: Equatable, Sendable {
  /// The most screenshots it shows.
  public static let mostShots = 3

  /// The host's icon.
  public var icon: RowIcon
  /// The session's name.
  public var name: String
  /// Its state and line, such as "Acting · has the helper".
  public var state: String
  /// The state's tone.
  public var tone: StatusTone
  /// Host, Folder, Started, and Last seen or Ended.
  public var facts: [Fact]
  /// Each app it opened: open, open from a copy, or closed.
  public var apps: [Fact]
  /// Each copy Atlas made for it.
  public var copies: [Fact]
  /// Each environment plan it applied, with where it stands.
  public var plans: [Fact]
  /// Its newest screenshots, at most three, with their thumbnails once read.
  public var shots: [ViewerActivityShot]
  /// "See all 6" under them; nil with three or fewer.
  public var allShots: String?

  /// Creates the session's detail.
  ///
  /// - Parameters:
  ///   - icon: The host's icon.
  ///   - name: The session's name.
  ///   - state: Its state and line.
  ///   - tone: The state's tone.
  ///   - facts: Host, Folder, Started, and Last seen or Ended.
  ///   - apps: Each app it opened; none by default.
  ///   - copies: Each copy made for it; none by default.
  ///   - plans: Each environment plan it applied; none by default.
  ///   - shots: Its newest screenshots; none by default.
  ///   - allShots: "See all 6"; nil by default.
  public init(
    icon: RowIcon,
    name: String,
    state: String,
    tone: StatusTone,
    facts: [Fact],
    apps: [Fact] = [],
    copies: [Fact] = [],
    plans: [Fact] = [],
    shots: [ViewerActivityShot] = [],
    allShots: String? = nil
  ) {
    self.icon = icon
    self.name = name
    self.state = state
    self.tone = tone
    self.facts = facts
    self.apps = apps
    self.copies = copies
    self.plans = plans
    self.shots = shots
    self.allShots = allShots
  }
}
