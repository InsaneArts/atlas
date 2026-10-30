import AtlasService
import Foundation

/// Finds apps' icons on the person's side, in order: from the app itself while it is open, then from the icons kept
/// between launches, then where its app is installed by bundle identifier, or for an app known only by name, as
/// `<Name>.app` where apps are installed. Every icon read from an app is kept, a changed one replacing the old; what
/// is not found is none, and its app draws its tile. A copy only the helper can read is found by its bundle
/// identifier where the person has the same app installed, and is otherwise its tile.
nonisolated struct AppIconSource: Sendable {
  /// The icons kept between launches.
  var store: AppIconStore
  /// The workspace's reads.
  var lookup: AppIconLookup
  /// The person's home folder, for their own Applications folder.
  var home: String

  /// Finds an icon, off the main actor.
  ///
  /// - Parameter want: The app whose icon is wanted.
  /// - Returns: The icon, or nil when none is found.
  @concurrent func find(_ want: AppIconWant) async -> AppIconFound? {
    if let live = await live(want) { return live }
    var nameKey: String?
    if case .name(let name)? = AppIconKey(name: want.name) { nameKey = name }
    var id = want.bundleId
    if id == nil, let nameKey { id = await store.bundleId(name: nameKey) }
    if let id {
      if let kept = await store.icon(bundleId: id) {
        guard kept.looksAgain, let fresh = await installed(id: id, name: want.name) else {
          return AppIconFound(image: kept.image, sha: kept.sha, bundleId: id)
        }
        return fresh
      }
      if let found = await installed(id: id, name: want.name) { return found }
      // An app known by its bundle identifier is never taken for another of the same name.
      guard want.bundleId == nil else { return nil }
    }
    return await installed(name: want.name, nameKey: nameKey)
  }

  /// The icon of an app open now, from its bundle where it is installed, else as the service sent it, kept under its
  /// bundle identifier.
  ///
  /// - Parameter want: The app.
  /// - Returns: The icon, or nil when the app is not open or neither reads.
  private func live(_ want: AppIconWant) async -> AppIconFound? {
    var png: Data?
    if let bundle = want.bundle { png = await lookup.icon(bundle) }
    guard let png = png ?? want.served else { return nil }
    return await kept(png, id: want.bundleId, name: want.name)
  }

  /// The icon of the app installed under a bundle identifier, kept.
  ///
  /// - Parameters:
  ///   - id: The bundle identifier.
  ///   - name: The app's name, if known.
  /// - Returns: The icon, or nil when no such app is installed where apps go.
  private func installed(id: String, name: String?) async -> AppIconFound? {
    guard let bundle = await lookup.bundleOfId(id), let png = await lookup.icon(bundle) else { return nil }
    return await kept(png, id: id, name: name)
  }

  /// The icon of `<Name>.app` where apps are installed, kept under the bundle identifier it declares, which its name
  /// then goes with.
  ///
  /// - Parameters:
  ///   - name: The app's name.
  ///   - nameKey: The name's key.
  /// - Returns: The icon, or nil when no app of that name is installed where apps go.
  private func installed(name: String?, nameKey: String?) async -> AppIconFound? {
    guard let name, let nameKey, let bundle = await lookup.bundleNamed(name), let png = await lookup.icon(bundle) else {
      return nil
    }
    let id = await lookup.bundleId(bundle)
    if let id { await store.learn(name: nameKey, bundleId: id) }
    return await kept(png, id: id, name: name)
  }

  /// An icon read from its app, kept when its bundle identifier is known, and decoded.
  ///
  /// - Parameters:
  ///   - png: The icon's PNG.
  ///   - id: The app's bundle identifier, if known.
  ///   - name: The app's name, if known.
  /// - Returns: The icon, or nil when it does not read as one.
  private func kept(_ png: Data, id: String?, name: String?) async -> AppIconFound? {
    if let id, let kept = await store.keep(png, bundleId: id, name: name) {
      return AppIconFound(image: kept.image, sha: kept.sha, bundleId: id)
    }
    guard let image = AppIconStore.picture(png) else { return nil }
    return AppIconFound(image: image, sha: AppIconStore.digest(png), bundleId: id)
  }
}
