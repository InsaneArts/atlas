import CoreGraphics

/// One question Atlas asks the person before an agent sends, as the panel's card and the viewer's banner draw it: who
/// asks and since when, what it would do, the task, what would be sent, the answers it offers, and what Allow for
/// This Task covers.
public struct AskCardState: Equatable, Sendable, Identifiable {
  /// The question's id, which an answer names.
  public var id: String
  /// The agent's mark.
  public var icon: RowIcon
  /// Who asks, such as "Claude Code · release".
  public var who: String
  /// How long it has waited, beside who asks, such as "0:42".
  public var waited: String
  /// What it would do, such as "Send a message in Slack".
  public var title: String
  /// The task line, such as "Task: post the 0.1.1 release notes"; nil hides it.
  public var task: String?
  /// What would be sent; nil shows nothing.
  public var body: AskCardBody?
  /// The answers offered, in order: Allow for This Task, Just This Once, Deny; Deny always.
  public var choices: [AskChoice]
  /// What Allow for This Task covers, such as "For this task covers Claude Code in Slack until it finishes or for 1
  /// hour."; nil while it is not offered.
  public var scope: String?
  /// What VoiceOver reads for the question as a whole.
  public var label: String

  /// The well's shape, its width over its height, which a picture is cut to: the panel's width inside the card at
  /// ``AskCardLayout/pictureHeight``.
  public static let pictureAspect = AskCardLayout.pictureAspect

  /// Creates a question's state.
  ///
  /// - Parameters:
  ///   - id: The question's id.
  ///   - icon: The agent's mark.
  ///   - who: Who asks.
  ///   - waited: How long it has waited.
  ///   - title: What it would do.
  ///   - task: The task line; none by default.
  ///   - body: What would be sent; nothing by default.
  ///   - choices: The answers offered; Deny is added when missing.
  ///   - scope: What Allow for This Task covers; none by default.
  ///   - label: What VoiceOver reads; the title by default.
  public init(
    id: String,
    icon: RowIcon,
    who: String,
    waited: String,
    title: String,
    task: String? = nil,
    body: AskCardBody? = nil,
    choices: [AskChoice] = AskChoice.allCases,
    scope: String? = nil,
    label: String? = nil
  ) {
    self.id = id
    self.icon = icon
    self.who = who
    self.waited = waited
    self.title = title
    self.task = task
    self.body = body
    let offered = Set(choices + [.deny])
    self.choices = AskChoice.allCases
      .filter { offered.contains($0) }
    self.scope = scope
    self.label = label ?? title
  }

  /// The main answer, which Return gives: Allow for This Task when offered, else Just This Once, else Deny.
  public var main: AskChoice {
    choices.first ?? .deny
  }

  /// The answers under the main one, in order.
  public var others: [AskChoice] {
    Array(choices.dropFirst())
  }
}
