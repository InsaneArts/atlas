import Foundation

/// One question Atlas asks the person before an agent sends from an account they listed: who asks and for what task,
/// the account, what the agent would do there, and, from `asks.json` alone, the message it would send and a picture
/// of the element it would press. The status's overview carries the same questions without what would be sent.
public struct Ask: Decodable, Equatable, Sendable, Identifiable {
  /// The question's id, which an answer names.
  public var id: String
  /// The key of the agent session asking.
  public var session: String
  /// Who asks, as the person reads it, such as "Claude Code · release".
  public var who: String
  /// The task, when the agent named one.
  public var task: String?
  /// The account, as the person reads it, such as "Slack" or "github.com".
  public var account: String
  /// What the agent would do there.
  public var kind: Kind
  /// The name of the element it would press, when it has one.
  public var element: String?
  /// The element's role, such as `AXButton`, when it was read.
  public var role: String?
  /// The key it would press, such as `return` or `cmd+return`.
  public var key: String?
  /// How many characters it would type.
  public var typed: Int
  /// The message a Return would send, when it was read whole; only `asks.json` carries it.
  public var message: Message?
  /// Whether Just This Once is offered: where it lands and what it sends could be bound.
  public var offersOnce: Bool
  /// Whether Allow for This Task is offered: which account it is, and the person's list, were read.
  public var offersTask: Bool
  /// When it was asked.
  public var asked: Date
  /// When it ends unanswered.
  public var ends: Date
  /// The picture of the screen before the action, as a file name in the asks folder, named anew each time a rerun
  /// replaces it, so it always goes with the frame read beside it; nil for none, and once the question is answered.
  public var picture: String?
  /// The element's frame in the picture, in its pixels; nil when unknown, as for an element action.
  public var frame: Area?
  /// The picture's size in pixels, published with the frame, which the frame is measured in; nil when unknown.
  public var pictureSize: ViewerStatus.Size?

  /// Creates a question.
  ///
  /// - Parameters:
  ///   - id: The question's id.
  ///   - session: The key of the agent session asking.
  ///   - who: Who asks.
  ///   - task: The task; none by default.
  ///   - account: The account, as the person reads it.
  ///   - kind: What the agent would do.
  ///   - element: The element's name; none by default.
  ///   - role: The element's role; none by default.
  ///   - key: The key; none by default.
  ///   - typed: How many characters it would type; none by default.
  ///   - message: The message it would send; none by default.
  ///   - offersOnce: Whether Just This Once is offered; yes by default.
  ///   - offersTask: Whether Allow for This Task is offered; yes by default.
  ///   - asked: When it was asked.
  ///   - ends: When it ends unanswered.
  ///   - picture: The picture's file name; none by default.
  ///   - frame: The element's frame in the picture; none by default.
  ///   - pictureSize: The picture's size in pixels; none by default.
  public init(
    id: String,
    session: String = "",
    who: String,
    task: String? = nil,
    account: String,
    kind: Kind,
    element: String? = nil,
    role: String? = nil,
    key: String? = nil,
    typed: Int = 0,
    message: Message? = nil,
    offersOnce: Bool = true,
    offersTask: Bool = true,
    asked: Date,
    ends: Date,
    picture: String? = nil,
    frame: Area? = nil,
    pictureSize: ViewerStatus.Size? = nil
  ) {
    self.id = id
    self.session = session
    self.who = who
    self.task = task
    self.account = account
    self.kind = kind
    self.element = element
    self.role = role
    self.key = key
    self.typed = typed
    self.message = message
    self.offersOnce = offersOnce
    self.offersTask = offersTask
    self.asked = asked
    self.ends = ends
    self.picture = picture
    self.frame = frame
    self.pictureSize = pictureSize
  }

  /// The fields of a question, and of the input service's part of it, as the service writes them.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case id, session, who, task, account, question, what, name, role, key, typed, message, once, asked, ends, picture,
      frame, pictureSize
  }

  /// Decodes a question, reading what the agent would do from its `question`, or, in the status's summary, from its
  /// own `what`. A missing answer flag offers that answer not, so a summary offers Deny alone.
  ///
  /// - Parameter decoder: The decoder reading one question.
  /// - Throws: `DecodingError` when it has no id.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    id = try container.decode(String.self, forKey: .id)
    session = container.lenientText(.session) ?? ""
    who = container.lenientText(.who) ?? "An agent"
    task = container.lenientText(.task)
    account = container.lenientText(.account) ?? "this account"
    let question = try? container.nestedContainer(keyedBy: CodingKeys.self, forKey: .question)
    let what = question?.lenientText(.what) ?? container.lenientText(.what)
    kind = what.flatMap(Kind.init(rawValue:)) ?? .unsure
    element = question?.lenientText(.name)
    role = question?.lenientText(.role)
    key = question?.lenientText(.key)
    typed = question?.lenientCount(.typed) ?? 0
    message = try? question?.decodeIfPresent(Message.self, forKey: .message)
    offersOnce = question?.lenientFlag(.once) ?? false
    offersTask = question?.lenientFlag(.task) ?? false
    asked = container.lenientDate(milliseconds: .asked) ?? .distantPast
    ends = container.lenientDate(milliseconds: .ends) ?? .distantPast
    picture = container.lenientText(.picture)
      .flatMap(Self.fileName)
    frame = try? question?.decodeIfPresent(Area.self, forKey: .frame)
    pictureSize = try? question?.decodeIfPresent(ViewerStatus.Size.self, forKey: .pictureSize)
  }

  /// A picture's name when it is one file's in the asks folder, never a path that leaves it.
  ///
  /// - Parameter name: The name the service gave.
  /// - Returns: The name, or nil when it holds a folder or leaves the folder.
  static func fileName(_ name: String) -> String? {
    guard !name.contains("/"), name != ".", name != ".." else { return nil }
    return name
  }

  /// Whether it is still up at `date`.
  ///
  /// - Parameter date: The time now.
  /// - Returns: Whether it has not ended yet.
  public func isOpen(at date: Date) -> Bool {
    ends > date
  }
}
