import AtlasService
import AtlasUI

extension ViewerPanesModel {
  /// 2: icons found at once, as thumbnails are read, so a first Apps reading of many apps fills steadily without
  /// crowding the workspace.
  static let iconsAtOnce = 2

  /// After an Apps reading: each open app's icon is wanted, read from the app itself once, and the bundle identifier
  /// each app's name goes with is kept. New live evidence retries a miss or waits behind a weaker lookup;
  /// evidence already processed or queued asks for nothing.
  ///
  /// - Parameter apps: The apps read.
  func wantIcons(for apps: [HelperApp]) {
    var wants: [AppIconWant] = []
    for app in apps {
      learn(app)
      guard let key = app.iconKey else { continue }
      guard let want = AppIconWant.open(app, home: icons.home, live: true),
        iconWork.readLive[key.text]?.covers(want) != true
      else { continue }
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
        seen.insert(want.key.text).inserted,
        panes.icons.image(bundleId: want.bundleId, name: entry.app ?? "") == nil
      else { return nil }
      return want
    }
    want(wants)
  }

  /// Coalesces requests by identity, retaining live evidence behind an in-flight lookup and retrying earlier misses.
  ///
  /// - Parameter wants: The icons wanted.
  private func want(_ wants: [AppIconWant]) {
    for incoming in wants {
      var want = incoming
      let key = want.key.text
      if want.isLive {
        iconWork.missed.remove(key)
        if let active = iconWork.active[key] {
          guard !active.covers(want) else { continue }
          want = want.keepingEvidence(from: active)
        }
      } else {
        guard iconWork.loading[key] == nil, !iconWork.missed.contains(key), panes.icons.images[key] == nil else {
          continue
        }
      }
      if let waiting = iconWork.wanted.firstIndex(where: { $0.key == want.key }) {
        // Keep every source already queued while accepting newer live evidence.
        if want.isLive { iconWork.wanted[waiting] = want.keepingEvidence(from: iconWork.wanted[waiting]) }
        continue
      }
      iconWork.wanted.append(want)
    }
    findIcons()
  }

  /// Starts the waiting icons, oldest first, while fewer than ``iconsAtOnce`` are being found. Each is found off the
  /// main actor.
  private func findIcons() {
    while iconWork.loading.count < Self.iconsAtOnce,
      let next = iconWork.wanted.firstIndex(where: { iconWork.loading[$0.key.text] == nil })
    {
      let want = iconWork.wanted.remove(at: next)
      iconWork.active[want.key.text] = want
      iconWork.loading[want.key.text] = Task { [weak self, icons] in
        let found = await icons.find(want)
        self?.land(found, for: want)
      }
    }
  }

  /// An icon found, or not: drawn from now on, or its app keeps its tile until new live evidence; the next one starts.
  /// An icon found under a bundle identifier for an app wanted by name is drawn for both.
  ///
  /// - Parameters:
  ///   - found: The icon, or nil when none was found.
  ///   - want: What was wanted.
  func land(_ found: AppIconFound?, for want: AppIconWant) {
    let key = want.key.text
    iconWork.loading[key] = nil
    iconWork.active[key] = nil
    if want.isLive { iconWork.readLive[key] = want }
    defer { findIcons() }
    guard let found else {
      if panes.icons.images[key] == nil { iconWork.missed.insert(key) }
      return
    }
    iconWork.missed.remove(key)
    let image = found.drawn(as: key)
    panes.icons.images[key] = image
    guard case .name = want.key, let id = found.bundleId, let bundleKey = AppIconKey(bundleId: id) else { return }
    panes.icons.names[key] = id
    if panes.icons.images[bundleKey.text] == nil { panes.icons.images[bundleKey.text] = image }
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
