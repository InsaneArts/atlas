import AtlasService
import AtlasStatus
import AtlasUI

extension PanelPresenter {
  /// The popup beside ⋯ while more than one desktop is set up: the title of the desktop the panel shows, and every
  /// desktop with its dot and state, the one shown checked, pressed while its menu is open; nil while one is, when the
  /// header is as it always was.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode, which the desktop shown is said by.
  /// - Returns: The popup, or nil.
  static func desktopPicker(_ snapshot: PanelSnapshot, mode: Mode) -> PanelDesktopPicker? {
    guard let title = DesktopItems.shownTitle(snapshot.desktops) else { return nil }
    return PanelDesktopPicker(
      title: title,
      items: DesktopItems.items(snapshot.desktops, shown: mode),
      isOpen: snapshot.session.desktopsOpen
    )
  }

  /// What the menu bar item shows: the most urgent state of every desktop, the shown one's mode as the panel sees it;
  /// the helper's mode alone while one desktop is set up.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The row and its word.
  static func urgent(_ snapshot: PanelSnapshot) -> DesktopItems.Urgent {
    DesktopItems.mostUrgent(snapshot.desktops, shown: mode(snapshot))
  }
}
