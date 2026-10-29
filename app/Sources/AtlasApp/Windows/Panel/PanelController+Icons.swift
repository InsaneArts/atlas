import AtlasService
import AtlasStatus
import AtlasUI

extension PanelController {
  /// Asks for the icon of each app Recent's rows name, once each a launch: found off the main actor, as the main
  /// window finds its own, from the icons kept between launches or where the app is installed. A row whose icon is
  /// not found keeps its app's tile.
  func wantRecentIcons() {
    for item in StatusReading(status).activity(limit: PanelPresenter.recentRows) {
      guard let want = AppIconWant.named(bundleId: item.row.bundleId, name: item.row.app),
        iconsAsked.insert(want.key.text).inserted
      else { continue }
      Task { [weak self, appIcons] in
        guard let found = await appIcons.find(want) else { return }
        self?.icons[want.key.text] = found.drawn(as: want.key.text)
      }
    }
  }
}
