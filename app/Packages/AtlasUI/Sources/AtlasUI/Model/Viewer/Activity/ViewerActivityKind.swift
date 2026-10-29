/// What kind of thing a step of the Activity pane did, which its disc's glyph shows: the kind is information, so each
/// has its own glyph; a problem and the person's own steps take theirs whatever they did.
public enum ViewerActivityKind: Equatable, Sendable, CaseIterable {
  /// A click, a drag or the pointer moved.
  case pointer
  /// Text typed.
  case typing
  /// Keys pressed.
  case keys
  /// A scroll.
  case scroll
  /// A screenshot taken.
  case screenshot
  /// An app, a file or a link opened, or apps closed.
  case window
  /// A command run, or anything else done in the helper.
  case command
  /// Something the person did.
  case person
  /// Control changing hands, such as given back.
  case control
  /// Something that did not work.
  case problem
}
