import AtlasService
import AtlasUI

extension ViewerPanesModel {
  /// After an Apps reading: each open app's icon is wanted, read from the app itself once, and the bundle identifier
  /// each app's name goes with is kept. New live evidence retries a miss or waits behind a weaker lookup;
  /// evidence already processed or queued asks for nothing.
  ///
  /// - Parameter apps: The apps read.
  func wantIcons(for apps: [HelperApp]) {
    var wants: [AppIconWant] = []
    for app in apps {
      learn(app)
      guard let want = AppIconWant.open(app, home: icons.home, live: true) else { continue }
      wants.append(want)
    }
    want(wants)
  }

  /// After an Activity reading: the icon of each app its rows name is wanted, once each.
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
    want(wants)
  }

  /// Keeps the viewer's request policy local while the app repository owns scheduling and caching. A reply from
  /// before the window closed or changed panes cannot write the newer state, and no task retains the window.
  private func want(_ wants: [AppIconWant]) {
    let previous = iconWants
    var latest: [String: AppIconWant] = [:]
    for want in wants {
      let key = want.key.text
      latest[key] = want.keepingEvidence(from: latest[key] ?? iconWants[key] ?? want)
    }
    iconWants = latest
    let keys = Set(wants.map { $0.key.text })
    // A pane owns its active images independently of the repository's reusable, bounded cache.
    panes.icons.images = panes.icons.images.filter { keys.contains($0.key) }
    for key in Array(iconRequests.keys) where !keys.contains(key) {
      iconRequests.removeValue(forKey: key)?
        .cancel()
    }
    let generation = work.generation
    for want in latest.values {
      let key = want.key.text
      guard iconRequests[key] == nil else {
        icons.repository.updateEvidence(want)
        continue
      }
      if panes.icons.images[key] != nil, !want.isLive || previous[key]?.covers(want) == true { continue }
      iconRequests[key] = icons.request(
        want,
        latest: { [weak self] in self?.iconWants[key] },
        receive: { [weak self] found, _ in
          guard let self, self.work.generation == generation else { return }
          self.iconRequests[key] = nil
          if let found { self.panes.icons.images[key] = found.drawn(as: key) }
          guard case .name = want.key, let id = found?.bundleId else { return }
          self.panes.icons.names[want.key.text] = id
        }
      )
    }
  }

  /// Keeps the bundle identifier an open app's name goes with, in the window and between launches, once a pair.
  ///
  /// - Parameter app: The app.
  private func learn(_ app: HelperApp) {
    guard let id = AppIconKey(bundleId: app.bundleId)?.bundleId, case .name(let name)? = AppIconKey(name: app.name)
    else { return }
    let key = AppIconKey.name(name).text
    if panes.icons.names[key] != id { panes.icons.names[key] = id }
    guard iconWork.learned[key] != id else { return }
    iconWork.learned[key] = id
    Task { [icons] in await icons.store.learn(name: name, bundleId: id) }
  }
}
