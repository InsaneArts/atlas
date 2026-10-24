/// Every SF Symbol Atlas draws, by name, so a state carries a symbol by value, never as a string literal.
/// Each is drawn at its text's weight in one of ``SymbolScale``'s sizes.
public enum SymbolName: String, CaseIterable, Sendable {
  /// Pause the picture.
  case pause = "pause.fill"
  /// Resume the picture.
  case play = "play.fill"
  /// Stop the agent, where a narrow window keeps the symbol alone.
  case stop = "stop.fill"
  /// Resume the agent, where a narrow window keeps the symbol alone; not play, which the picture's pause becomes.
  case resume = "forward.fill"
  /// Stop showing the window outlined on the screen, in its label.
  case clear = "xmark"
  /// Empty a search field.
  case clearText = "xmark.circle.fill"
  /// Keys, while in control.
  case keyboard
  /// More.
  case more = "ellipsis"
  /// Details, the inspector.
  case sidebar = "sidebar.right"
  /// Expand the small screen.
  case expand = "arrow.up.left.and.arrow.down.right"
  /// A search field.
  case search = "magnifyingglass"
  /// Show on Screen.
  case show = "eye"
  /// Check again.
  case reload = "arrow.clockwise"
  /// The previous screenshot.
  case back = "chevron.left"
  /// The next screenshot, and the helper's allow pairing: the asking account handing over to Atlas.
  case next = "chevron.right"
  /// Copy.
  case copy = "doc.on.doc"
  /// Show in Finder, and a project.
  case folder
  /// A click.
  case click = "cursorarrow.click"
  /// A scroll.
  case scroll = "arrow.up.and.down"
  /// A screenshot.
  case camera
  /// An app opened, or a window.
  case window = "macwindow"
  /// A command, or Codex.
  case terminal
  /// Something the person did.
  case person = "person.fill"
  /// Control given back.
  case handRaised = "hand.raised"
  /// A problem.
  case problem = "exclamationmark.triangle.fill"
  /// A problem's round mark, in a status line, a checklist or a row's word.
  case attention = "exclamationmark.circle.fill"
  /// A problem hero's bare mark, on its orange-tinted disc.
  case exclamation = "exclamationmark"
  /// A check that passed.
  case passed = "checkmark.circle.fill"
  /// The General settings.
  case gear = "gearshape"
  /// The helper's screen, and the narrow Screen tab.
  case display
  /// The agents, and an update available.
  case sparkles
  /// Connected coding agents as a Settings category.
  case agents = "puzzlepiece.extension.fill"
  /// About.
  case info = "info.circle"
  /// The narrow Activity tab.
  case list = "list.bullet"
  /// The narrow Apps tab.
  case windows = "macwindow.on.rectangle"
  /// The narrow Screenshots tab.
  case photos = "photo.on.rectangle"
  /// Something locked.
  case lock
  /// A popup button's choices.
  case popup = "chevron.up.chevron.down"
  /// Screen Recording: the area a screenshot takes.
  case screenArea = "rectangle.dashed"
  /// The drag ghost's hand, its edge.
  case hand = "hand.point.up.left"
  /// The drag ghost's hand, its fill.
  case handFill = "hand.point.up.left.fill"
}
