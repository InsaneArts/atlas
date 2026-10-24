/// One line of the Activity pane: when it started, its mark and its kind, what was done (with a count for repeats)
/// and why it failed, and the screenshot it took.
public struct ViewerActivityLine: Equatable, Sendable, Identifiable {
  /// The line's identity: its earliest row's id, never reused.
  public var id: String
  /// When it started, such as "10:31:42"; a dash when unknown.
  public var time: String
  /// Its mark: whether it runs now, failed, or was the person's own, which tints its tile.
  public var lead: ActivityMarkKind
  /// What kind of thing it did, which its tile's symbol shows.
  public var kind: ViewerActivityKind
  /// What was done, such as "You clicked in Opie ×3".
  public var text: String
  /// Why it did not work, under it; nil for none.
  public var reason: String?
  /// The screenshot it took; nil for every line but a screenshot's.
  public var shot: ViewerActivityShot?
  /// What VoiceOver reads for the line.
  public var label: String

  /// Creates a line.
  ///
  /// - Parameters:
  ///   - id: Its earliest row's id.
  ///   - time: When it started.
  ///   - lead: Its mark.
  ///   - kind: What kind of thing it did.
  ///   - text: What was done.
  ///   - reason: Why it did not work.
  ///   - shot: The screenshot it took.
  ///   - label: What VoiceOver reads for it.
  public init(
    id: String,
    time: String,
    lead: ActivityMarkKind,
    kind: ViewerActivityKind,
    text: String,
    reason: String? = nil,
    shot: ViewerActivityShot? = nil,
    label: String
  ) {
    self.id = id
    self.time = time
    self.lead = lead
    self.kind = kind
    self.text = text
    self.reason = reason
    self.shot = shot
    self.label = label
  }
}
