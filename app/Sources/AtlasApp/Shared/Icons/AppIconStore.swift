import AtlasService
import Foundation

/// The app icons kept between launches, in `~/Library/Caches/Atlas/AppIcons/v1`: one PNG an app, by its bundle
/// identifier, and the index (``AppIconIndex``) beside them. Nothing is read until an icon is first wanted; then the
/// index is read once, and an icon's file only when a window asks for it. Every write goes to a new file in the folder
/// that is then renamed over the old, so a file is never seen half written; the folders are the person's alone (0700)
/// and the files too (0600). An index that does not read starts the cache over, and past its limits the least
/// recently used icons go (``AppIconCachePolicy``). The cache holds a bundle identifier, an app's name, its icon and
/// the day it was last used: no action, time of day, title or path. Only its own folder is made or changed; the
/// rest of Atlas's caches, the locks among them, are never touched. With no folder, as in tests and renders, it
/// keeps nothing.
actor AppIconStore {
  /// The store's own folder inside Atlas's caches, by its shape.
  static let folderPath = ["AppIcons", "v1"]
  /// The index's file name.
  static let indexName = "index.json"
  /// What starts a file still being written.
  static let partPrefix = ".part-"
  /// 0700: the folders, the person's alone.
  static let folderMode = 0o700
  /// 0600: the files, the person's alone.
  static let fileMode = 0o600

  /// Atlas's caches folder, which the store makes its own folder in; nil keeps nothing.
  private let caches: URL?
  /// Today's day, as ``AppIconCachePolicy/day(of:)`` counts it.
  private let today: @Sendable () -> Int
  /// The index, once read; nil before the first icon is wanted.
  private var index: AppIconIndex?
  /// The store's folder, once made; nil before the first icon is wanted, or when it could not be made.
  private var folder: URL?
  /// Whether the folder was made, or tried, so a folder that could not be made is not tried again with each icon.
  private var opened = false

  /// Creates a store in Atlas's caches.
  ///
  /// - Parameters:
  ///   - caches: Atlas's caches folder, such as `~/Library/Caches/Atlas`; nil keeps nothing.
  ///   - today: Today's day.
  init(caches: URL?, today: @escaping @Sendable () -> Int = { AppIconCachePolicy.day(of: Date()) }) {
    self.caches = caches
    self.today = today
  }

  /// The store in the person's own Atlas caches.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The store.
  static func live(home: URL) -> AppIconStore {
    AppIconStore(
      caches: home.appendingPathComponent("Library/Caches/Atlas", isDirectory: true)
    )
  }

  /// An app's icon as kept, by its bundle identifier, marked used today. A file that is missing, too large or does not
  /// read as a picture within ``AppIconCachePolicy/largestSide`` is dropped from the cache.
  ///
  /// - Parameter bundleId: The app's bundle identifier.
  /// - Returns: The icon and whether it is due to be looked up again, or nil when none is kept.
  func icon(bundleId: String) -> AppIconStore.Kept? {
    guard let folder = loaded(), let entry = index?.icons[bundleId] else { return nil }
    guard let image = Self.read(folder.appendingPathComponent(entry.file), bytes: entry.bytes) else {
      forget([bundleId], in: folder)
      save(in: folder)
      return nil
    }
    let day = today()
    if entry.used != day {
      index?.icons[bundleId]?.used = day
      save(in: folder)
    }
    return Kept(image: image, sha: entry.sha, looksAgain: AppIconCachePolicy.looksAgain(entry, today: day))
  }

  /// The bundle identifier an app's name was last seen with.
  ///
  /// - Parameter name: The name's key (``AppIconKey/name(_:)``).
  /// - Returns: The bundle identifier, or nil.
  func bundleId(name: String) -> String? {
    guard loaded() != nil else { return nil }
    return index?.names[name]
  }

  /// Keeps that an app's name goes with a bundle identifier, written only when it is new.
  ///
  /// - Parameters:
  ///   - name: The name's key.
  ///   - bundleId: The bundle identifier.
  func learn(name: String, bundleId: String) {
    guard let folder = loaded(), AppIconKey.isBundleId(bundleId), index?.names[name] != bundleId else { return }
    index?.names[name] = bundleId
    if let index { forgetNames(AppIconCachePolicy.forgottenNames(index)) }
    save(in: folder)
  }

  /// Keeps an icon read from its app, replacing the one kept when it changed: a PNG within
  /// ``AppIconCachePolicy/largestFile`` that reads as a picture within ``AppIconCachePolicy/largestSide``. The same
  /// icon read again only marks it read and used today.
  ///
  /// - Parameters:
  ///   - png: The icon's PNG.
  ///   - bundleId: The app's bundle identifier.
  ///   - name: The app's name, if known.
  /// - Returns: The icon as kept, or nil when it is refused.
  @discardableResult func keep(_ png: Data, bundleId: String, name: String?) -> AppIconStore.Kept? {
    guard AppIconKey.isBundleId(bundleId), let image = Self.picture(png) else { return nil }
    let sha = Self.digest(png)
    let kept = Kept(image: image, sha: sha, looksAgain: false)
    guard let folder = loaded() else { return kept }
    let day = today()
    let boundedName = name.map { String($0.prefix(AppIconKey.longestName)) }
    if var entry = index?.icons[bundleId], entry.sha == sha {
      entry.used = day
      entry.read = day
      entry.name = boundedName ?? entry.name
      guard entry != index?.icons[bundleId] else { return kept }
      index?.icons[bundleId] = entry
      save(in: folder)
      return kept
    }
    let file = Self.fileName(bundleId)
    guard Self.write(png, to: folder.appendingPathComponent(file), in: folder) else { return kept }
    index?.icons[bundleId] = AppIconIndex.Entry(
      file: file,
      name: boundedName,
      used: day,
      read: day,
      bytes: png.count,
      sha: sha
    )
    if let index { forget(AppIconCachePolicy.evicted(index), in: folder) }
    save(in: folder)
    return kept
  }

  /// The folder, made and its index read on the first call; nil when there is no folder to keep icons in.
  ///
  /// - Returns: The folder.
  private func loaded() -> URL? {
    guard !opened else { return folder }
    opened = true
    guard let caches, let made = Self.made(caches) else { return nil }
    folder = made
    let read = Self.readIndex(in: made)
    if read == nil { Self.empty(made) }
    var index = read ?? AppIconIndex()
    Self.sweep(made, keeping: Set(index.icons.values.map(\.file)))
    let evicted = AppIconCachePolicy.evicted(index)
    for id in evicted {
      if let file = index.icons.removeValue(forKey: id)?.file { Self.remove(made.appendingPathComponent(file)) }
    }
    self.index = index
    if read != index { save(in: made) }
    return made
  }

  /// Drops icons from the index and their files from the folder.
  ///
  /// - Parameters:
  ///   - ids: The bundle identifiers.
  ///   - folder: The store's folder.
  private func forget(_ ids: [String], in folder: URL) {
    for id in ids {
      guard let entry = index?.icons.removeValue(forKey: id) else { continue }
      Self.remove(folder.appendingPathComponent(entry.file))
    }
  }

  /// Drops names from the index.
  ///
  /// - Parameter names: The names' keys.
  private func forgetNames(_ names: [String]) {
    for name in names { index?.names[name] = nil }
  }

  /// Writes the index; a write that fails leaves the last one, which the next write replaces.
  ///
  /// - Parameter folder: The store's folder.
  private func save(in folder: URL) {
    guard let index else { return }
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    guard let data = try? encoder.encode(index) else { return }
    Self.write(data, to: folder.appendingPathComponent(Self.indexName), in: folder)
  }
}
