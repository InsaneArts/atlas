import Foundation

/// What Atlas recorded about an agent's action beside its pictures, so the app can mark the spot: the kind of input,
/// how it went, the display the picture shows, where it acted, and what it sent. Read only by the app; typed text may
/// already be shortened by the person's setting, and a password's text and length are never recorded.
public struct ActionFacts: Decodable, Equatable, Sendable {
  /// What kind of input it sent; nil for a kind this app does not know.
  public var action: Action?
  /// How it went; nil when unknown.
  public var outcome: Outcome?
  /// Why it failed or what held it, in plain words.
  public var reason: String?
  /// The display the picture shows, in points; the picture's pixels per point are its width over this width.
  public var display: Size?
  /// Where it acted, in points from the display's top-left corner.
  public var point: Point?
  /// Where a drag ended.
  public var to: Point?
  /// How many times it clicked.
  public var clicks: Int?
  /// Which button it clicked with: `left`, `right` or `middle`.
  public var button: String?
  /// The keys it pressed, one chord's caps each, such as `[["⌘", "S"]]`.
  public var keys: [[String]]
  /// What it typed, perhaps shortened; nil for a password, or when the field is unknown.
  public var text: String?
  /// How many characters it typed; nil for a password.
  public var length: Int?
  /// Whether it typed into a password field.
  public var secure: Bool
  /// How far it scrolled.
  public var scroll: Scroll?
  /// The control or field it named.
  public var label: String?
  /// The element it went to, where it stands and what it is; nil when unknown.
  public var element: Element?
  /// The task the agent named for the command that sent it; nil when it named none.
  public var task: String?
  /// When the picture before it was taken.
  public var at: Date?
  /// When the action replied.
  public var endedAt: Date?

  /// Creates an action's facts.
  ///
  /// - Parameters:
  ///   - action: What kind of input it sent.
  ///   - outcome: How it went.
  ///   - reason: Why it failed or what held it.
  ///   - display: The display the picture shows, in points.
  ///   - point: Where it acted.
  ///   - to: Where a drag ended.
  ///   - clicks: How many times it clicked.
  ///   - button: Which button it clicked with.
  ///   - keys: The keys it pressed.
  ///   - text: What it typed.
  ///   - length: How many characters it typed.
  ///   - secure: Whether it typed into a password field.
  ///   - scroll: How far it scrolled.
  ///   - label: The control or field it named.
  ///   - element: The element it went to.
  ///   - task: The task the agent named.
  ///   - at: When the picture before it was taken.
  ///   - endedAt: When the action replied.
  public init(
    action: Action? = nil,
    outcome: Outcome? = nil,
    reason: String? = nil,
    display: Size? = nil,
    point: Point? = nil,
    to: Point? = nil,
    clicks: Int? = nil,
    button: String? = nil,
    keys: [[String]] = [],
    text: String? = nil,
    length: Int? = nil,
    secure: Bool = false,
    scroll: Scroll? = nil,
    label: String? = nil,
    element: Element? = nil,
    task: String? = nil,
    at: Date? = nil,
    endedAt: Date? = nil
  ) {
    self.action = action
    self.outcome = outcome
    self.reason = reason
    self.display = display
    self.point = point
    self.to = to
    self.clicks = clicks
    self.button = button
    self.keys = keys
    self.text = text
    self.length = length
    self.secure = secure
    self.scroll = scroll
    self.label = label
    self.element = element
    self.task = task
    self.at = at
    self.endedAt = endedAt
  }

  /// The fields of `facts` this app reads.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case action, outcome, reason, display, point, to, clicks, button, keys, text, length, secure, scroll, label, element
    /// Each field by the name the service writes.
    case task, at
    /// When the action replied.
    case endedAt
  }

  /// Decodes an action's facts, each field on its own: one missing or odd reads as none, and never drops the rest. A
  /// password's text is never kept, whatever the service wrote.
  ///
  /// - Parameter decoder: The decoder reading `facts`.
  /// - Throws: `DecodingError` only when `facts` is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    action = container.lenientText(.action)
      .flatMap(Action.init(rawValue:))
    outcome = container.lenientText(.outcome)
      .flatMap(Outcome.init(rawValue:))
    reason = container.lenientText(.reason)
    display = try? container.decodeIfPresent(Size.self, forKey: .display)
    point = try? container.decodeIfPresent(Point.self, forKey: .point)
    to = try? container.decodeIfPresent(Point.self, forKey: .to)
    clicks = (try? container.decodeIfPresent(Int.self, forKey: .clicks))
      .flatMap { $0 > 0 ? $0 : nil }
    button = container.lenientText(.button)
    keys = (try? container.decodeIfPresent([[String]].self, forKey: .keys)) ?? []
    secure = container.lenientFlag(.secure)
    text = secure ? nil : container.lenientText(.text)
    let length = (try? container.decodeIfPresent(Int.self, forKey: .length)) ?? nil
    self.length = secure ? nil : length.flatMap { $0 >= 0 ? $0 : nil }
    scroll = try? container.decodeIfPresent(Scroll.self, forKey: .scroll)
    label = container.lenientText(.label)
    element = try? container.decodeIfPresent(Element.self, forKey: .element)
    task = container.lenientText(.task)
    at = container.lenientDate(milliseconds: .at)
    endedAt = container.lenientDate(milliseconds: .endedAt)
  }
}
