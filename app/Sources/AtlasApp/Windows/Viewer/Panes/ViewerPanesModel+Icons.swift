import AtlasService
import AtlasUI

extension ViewerPanesModel {
  /// After an Activity reading: the icon of each app its rows name is wanted, once each. A reply from before the
  /// window closed or changed panes cannot write the newer state, and no task retains the window; the app repository
  /// owns scheduling and caching.
  ///
  /// - Parameter entries: The rows read.
  func wantIcons(for entries: [ActivityLog.Entry]) {
    var seen: Set<String> = []
    let wants = entries.compactMap { entry -> AppIconWant? in
      guard let want = AppIconWant.named(bundleId: entry.bundleId, name: entry.app),
        seen.insert(want.key.text).inserted
      else { return nil }
      return want
    }
    let keys = Set(wants.map { $0.key.text })
    // A pane owns its active images independently of the repository's reusable, bounded cache.
    panes.icons.images = panes.icons.images.filter { keys.contains($0.key) }
    for key in Array(iconRequests.keys) where !keys.contains(key) {
      iconRequests.removeValue(forKey: key)?
        .cancel()
    }
    let generation = work.generation
    for want in wants {
      let key = want.key.text
      guard iconRequests[key] == nil, panes.icons.images[key] == nil else { continue }
      iconRequests[key] = icons.request(want) { [weak self] found, _ in
        guard let self, self.work.generation == generation else { return }
        self.iconRequests[key] = nil
        if let found { self.panes.icons.images[key] = found.drawn(as: key) }
        guard case .name = want.key, let id = found?.bundleId else { return }
        self.panes.icons.names[want.key.text] = id
      }
    }
  }
}
