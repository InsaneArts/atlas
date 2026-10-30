import AtlasService
import AtlasStatus
import AtlasUI

extension PanelController {
  /// Recent chooses its own apps; the shared repository coalesces their lookups with the viewer's and remembers
  /// misses. Each reading refreshes its bounded snapshot, including live icons the viewer has since supplied.
  func wantRecentIcons() {
    let wants =
      isOpen
      ? StatusReading(status)
        .activity(limit: PanelPresenter.recentRows)
        .compactMap { AppIconWant.named(bundleId: $0.row.bundleId, name: $0.row.app) }
      : []
    let keys = Set(wants.map { $0.key.text })
    for key in Array(iconRequests.keys) where !keys.contains(key) {
      iconRequests.removeValue(forKey: key)?
        .cancel()
    }
    for want in wants {
      let key = want.key.text
      guard iconRequests[key] == nil else { continue }
      iconRequests[key] = appIcons.request(want) { [weak self] _, images in
        guard let self, self.isOpen else { return }
        self.iconRequests[key] = nil
        self.icons = images
      }
    }
  }
}
