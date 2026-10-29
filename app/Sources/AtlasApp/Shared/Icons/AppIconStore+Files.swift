import AtlasService
import CryptoKit
import Foundation
import ImageIO

extension AppIconStore {
  /// The store's folder inside Atlas's caches, made the person's alone when missing. Atlas's caches folder is made
  /// as Atlas's command line makes it, and only when missing; a link anywhere on the way is refused, since icons kept
  /// through a link would land in whatever folder it names.
  ///
  /// - Parameter caches: Atlas's caches folder.
  /// - Returns: The folder, or nil when it cannot be made.
  nonisolated static func made(_ caches: URL) -> URL? {
    let files = FileManager.default
    if !exists(caches) {
      // One that cannot be made is refused just below.
      try? files.createDirectory(at: caches, withIntermediateDirectories: true)
    }
    guard isFolder(caches) else { return nil }
    var folder = caches
    for part in folderPath {
      folder = folder.appendingPathComponent(part, isDirectory: true)
      if !exists(folder) {
        let attributes: [FileAttributeKey: Any] = [.posixPermissions: folderMode]
        // One that cannot be made is refused just below.
        try? files.createDirectory(at: folder, withIntermediateDirectories: false, attributes: attributes)
      }
      guard isFolder(folder) else { return nil }
    }
    return folder
  }

  /// The index as written, checked (``AppIconIndex/checked``); nil when it is missing, too large, does not read or is
  /// of another shape.
  ///
  /// - Parameter folder: The store's folder.
  /// - Returns: The index, or nil.
  nonisolated static func readIndex(in folder: URL) -> AppIconIndex? {
    let file = folder.appendingPathComponent(indexName)
    guard let data = contents(of: file, largest: AppIconCachePolicy.largestIndex) else { return nil }
    return (try? JSONDecoder().decode(AppIconIndex.self, from: data))?
      .checked
  }

  /// Removes everything in the store's folder, so the cache starts over.
  ///
  /// - Parameter folder: The store's folder.
  nonisolated static func empty(_ folder: URL) {
    for item in items(in: folder) { remove(item) }
  }

  /// Removes what the index does not name: files left half written and icons no entry keeps.
  ///
  /// - Parameters:
  ///   - folder: The store's folder.
  ///   - kept: The icons' file names the index keeps.
  nonisolated static func sweep(_ folder: URL, keeping kept: Set<String>) {
    for item in items(in: folder) where item.lastPathComponent != indexName && !kept.contains(item.lastPathComponent) {
      remove(item)
    }
  }

  /// The items directly in a folder.
  ///
  /// - Parameter folder: The folder.
  /// - Returns: The items, or none when it cannot be read.
  nonisolated static func items(in folder: URL) -> [URL] {
    (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
  }

  /// Removes a file, or says nothing when it is already gone.
  ///
  /// - Parameter file: The file.
  nonisolated static func remove(_ file: URL) {
    try? FileManager.default
      .removeItem(at: file)
  }

  /// Writes data as one file at once: to a new file in the folder, the person's alone, then renamed over the old.
  ///
  /// - Parameters:
  ///   - data: The data.
  ///   - file: The file it replaces.
  ///   - folder: The folder both are in.
  /// - Returns: Whether it was written.
  @discardableResult nonisolated static func write(_ data: Data, to file: URL, in folder: URL) -> Bool {
    let part = folder.appendingPathComponent(partPrefix + UUID().uuidString)
    let attributes: [FileAttributeKey: Any] = [.posixPermissions: fileMode]
    let files = FileManager.default
    guard files.createFile(atPath: part.path, contents: data, attributes: attributes) else { return false }
    guard Foundation.rename(part.path, file.path) == 0 else {
      remove(part)
      return false
    }
    return true
  }

  /// An icon's file as a picture, when it is the size the index says and decodes within
  /// ``AppIconCachePolicy/largestSide``.
  ///
  /// - Parameters:
  ///   - file: The icon's file.
  ///   - bytes: Its size, as the index says.
  /// - Returns: The picture, or nil.
  nonisolated static func read(_ file: URL, bytes: Int) -> CGImage? {
    guard let data = contents(of: file, largest: AppIconCachePolicy.largestFile), data.count == bytes else {
      return nil
    }
    return picture(data)
  }

  /// A PNG within ``AppIconCachePolicy/largestFile`` as a picture within ``AppIconCachePolicy/largestSide``, its size
  /// read before it is decoded, and decoded now so it never decodes on the main thread when drawn.
  ///
  /// - Parameter png: The PNG.
  /// - Returns: The picture, or nil.
  nonisolated static func picture(_ png: Data) -> CGImage? {
    guard AppIconCachePolicy.accepts(bytes: png.count),
      let source = CGImageSourceCreateWithData(png as CFData, nil),
      let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
      let width = properties[kCGImagePropertyPixelWidth] as? Int,
      let height = properties[kCGImagePropertyPixelHeight] as? Int,
      AppIconCachePolicy.accepts(width: width, height: height)
    else { return nil }
    let options = [kCGImageSourceShouldCacheImmediately: true, kCGImageSourceShouldCache: true] as CFDictionary
    return CGImageSourceCreateImageAtIndex(source, 0, options)
  }

  /// A file's bytes, when it is a plain file no larger than `largest`; a link or a folder is not read.
  ///
  /// - Parameters:
  ///   - file: The file.
  ///   - largest: The most bytes read.
  /// - Returns: The bytes, or nil.
  nonisolated static func contents(of file: URL, largest: Int) -> Data? {
    guard let attributes = try? FileManager.default.attributesOfItem(atPath: file.path),
      attributes[.type] as? FileAttributeType == .typeRegular,
      let size = attributes[.size] as? Int,
      size <= largest
    else { return nil }
    return try? Data(contentsOf: file)
  }

  /// Whether something is at a path, a link included, without following it.
  ///
  /// - Parameter url: The path.
  /// - Returns: Whether anything is there.
  nonisolated static func exists(_ url: URL) -> Bool {
    (try? FileManager.default.attributesOfItem(atPath: url.path)) != nil
  }

  /// Whether a path is a folder itself, not a link to one.
  ///
  /// - Parameter url: The path.
  /// - Returns: Whether it is a folder.
  nonisolated static func isFolder(_ url: URL) -> Bool {
    let attributes = try? FileManager.default
      .attributesOfItem(atPath: url.path)
    return attributes?[.type] as? FileAttributeType == .typeDirectory
  }

  /// An icon's file name: the first ``AppIconIndex/Entry/fileDigits`` hexadecimal digits of its bundle identifier's
  /// SHA-256, so the folder's listing names no app.
  ///
  /// - Parameter bundleId: The bundle identifier.
  /// - Returns: The file name.
  nonisolated static func fileName(_ bundleId: String) -> String {
    String(digest(Data(bundleId.utf8)).prefix(AppIconIndex.Entry.fileDigits)) + AppIconIndex.Entry.fileSuffix
  }

  /// Data's SHA-256, in lowercase hexadecimal.
  ///
  /// - Parameter data: The data.
  /// - Returns: The digest.
  nonisolated static func digest(_ data: Data) -> String {
    SHA256.hash(data: data)
      .map { String(format: "%02x", $0) }
      .joined()
  }
}
