/// What kind of thing an Activity line did, which its tile's symbol shows: the kind is information, so each
/// has its own symbol; a problem and the person's own lines take theirs whatever they did.
public enum ViewerActivityKind: Equatable, Sendable, CaseIterable {
  /// A click, a drag or the pointer moved.
  case pointer
  /// Text typed or keys pressed.
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
