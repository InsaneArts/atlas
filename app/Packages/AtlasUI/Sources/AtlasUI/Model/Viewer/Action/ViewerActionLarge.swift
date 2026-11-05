import CoreGraphics

/// One action shown large over the whole window: what was done and by whom, the screen before it with its mark and
/// the screen after it to compare, the details Atlas knows, and where Previous and Next go.
public struct ViewerActionLarge: Equatable, Sendable {
  /// The action's row, which Previous and Next move from.
  public var id: String
  /// What was done, as its step says it.
  public var words: ViewerStepWords
  /// Who did it, in which task and app, and which step of how many, such as "Claude Code · atlas · Post the 0.1.1
  /// release note · Slack · step 5 of 5".
  public var context: String
  /// The icon of the agent that did it; nil when unknown.
  public var icon: RowIcon?
  /// The screen before it, as taken once read, its thumbnail before then.
  public var before: DesktopPicture?
  /// The screen after it, as taken once read; nil before then or when there is none.
  public var after: DesktopPicture?
  /// Whether there is a picture after it to compare with.
  public var hasAfter: Bool
  /// The pictures' shape, which their frame keeps.
  public var aspect: CGSize
  /// The mark over the pictures; nil when nothing was recorded to place one.
  public var mark: ViewerActionMark?
  /// The label over the picture before, such as "Before · 10:31:09.21".
  public var beforeLabel: String
  /// The label over the picture after, such as "After · 10:31:10.02".
  public var afterLabel: String
  /// What Atlas knows of it, line by line.
  public var details: [ViewerActionDetail]
  /// How the two pictures are compared.
  public var compare: ViewerCompareMode
  /// Whether what stayed the same is dimmed.
  public var showsChanges: Bool
  /// The squares that changed between the two, as shares of the picture; nil until compared.
  public var changes: [CGRect]?
  /// Whether Previous goes anywhere.
  public var canGoBack: Bool
  /// Whether Next goes anywhere.
  public var canGoForward: Bool

  /// Creates the large view's state.
  ///
  /// - Parameters:
  ///   - id: The action's row.
  ///   - words: What was done.
  ///   - context: Who did it, where, and which step.
  ///   - icon: The agent's icon; none by default.
  ///   - before: The screen before it.
  ///   - after: The screen after it; none by default.
  ///   - hasAfter: Whether there is a picture after it.
  ///   - aspect: The pictures' shape.
  ///   - mark: The mark over the pictures; none by default.
  ///   - beforeLabel: The label over the picture before.
  ///   - afterLabel: The label over the picture after; none by default.
  ///   - details: What Atlas knows of it.
  ///   - compare: How the pictures are compared; slide by default.
  ///   - showsChanges: Whether what stayed the same is dimmed; it is not by default.
  ///   - changes: The squares that changed; unknown by default.
  ///   - canGoBack: Whether Previous goes anywhere.
  ///   - canGoForward: Whether Next goes anywhere.
  public init(
    id: String,
    words: ViewerStepWords,
    context: String,
    icon: RowIcon? = nil,
    before: DesktopPicture?,
    after: DesktopPicture? = nil,
    hasAfter: Bool,
    aspect: CGSize,
    mark: ViewerActionMark? = nil,
    beforeLabel: String,
    afterLabel: String = "",
    details: [ViewerActionDetail],
    compare: ViewerCompareMode = .slide,
    showsChanges: Bool = false,
    changes: [CGRect]? = nil,
    canGoBack: Bool,
    canGoForward: Bool
  ) {
    self.id = id
    self.words = words
    self.context = context
    self.icon = icon
    self.before = before
    self.after = after
    self.hasAfter = hasAfter
    self.aspect = aspect
    self.mark = mark
    self.beforeLabel = beforeLabel
    self.afterLabel = afterLabel
    self.details = details
    self.compare = compare
    self.showsChanges = showsChanges
    self.changes = changes
    self.canGoBack = canGoBack
    self.canGoForward = canGoForward
  }

  /// How the pictures are compared as drawn: the picture before alone while there is none after.
  public var shownCompare: ViewerCompareMode {
    hasAfter ? compare : .before
  }
}
