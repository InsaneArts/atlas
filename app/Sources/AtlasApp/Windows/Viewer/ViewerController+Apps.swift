import AtlasKit
import AtlasUI

/// The Apps tab: its reading, each app's icon decoded once off the main actor, and Show on screen, which outlines a
/// window on the Screen tab and acts on nothing in the helper.
extension ViewerController {
  /// Reads the apps and decodes the icons not known yet. A reply overtaken by a newer start or stop has no effect; a
  /// failure is kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readApps(_ number: Int) async {
    do {
      let apps = try await service.apps()
      let icons = await Self.newIcons(apps, known: Set(panes.apps.icons.keys))
      guard number == paneWork.generation else { return }
      panes.apps.apps = apps
      panes.apps.icons.merge(icons) { _, new in new }
      panes.apps.failure = nil
    } catch {
      guard number == paneWork.generation, !error.isCancelled else { return }
      panes.apps.failure = error.message
    }
  }

  /// The icons of `apps` not yet known, decoded off the main actor.
  ///
  /// - Parameters:
  ///   - apps: The apps read.
  ///   - known: The icons decoded already, by what they are known by.
  /// - Returns: The new icons, by what they are known by.
  static func newIcons(_ apps: [HelperApp], known: Set<String>) async -> [String: IconImage] {
    var icons: [String: IconImage] = [:]
    for app in apps where !known.contains(app.iconID) && icons[app.iconID] == nil {
      guard let png = app.icon, let image = await ImageDecoding.image(png) else { continue }
      icons[app.iconID] = IconImage(id: app.iconID, image: image)
    }
    return icons
  }

  /// Show on screen: outlines a window as it is now and shows the Screen tab. View-only: nothing goes to the helper.
  ///
  /// - Parameters:
  ///   - pid: The app's process id.
  ///   - id: The window's number.
  func showWindow(app pid: Int, window id: Int) {
    guard let app = panes.apps.apps?.first(where: { $0.pid == pid }),
      let window = app.windows.first(where: { $0.id == id })
    else { return }
    panes.apps.shown = ViewerShownWindow(app: app.name, icon: app.icon == nil ? nil : app.iconID, window: window)
    selectTab(.screen)
  }
}
