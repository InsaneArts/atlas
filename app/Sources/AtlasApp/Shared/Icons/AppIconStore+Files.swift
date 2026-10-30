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
  nonisolated static func made(_ caches: URL) -> IconCacheDirectory? {
    IconCacheDirectory.open(caches, appending: folderPath)
  }

  /// The checked index, read relative to the verified cache handle.
  ///
  /// - Parameter folder: The verified store directory.
  /// - Returns: The index, or nil when it cannot be read or checked.
  nonisolated static func readIndex(in folder: IconCacheDirectory) -> AppIconIndex? {
    guard let data = folder.read(indexName, largest: AppIconCachePolicy.largestIndex) else { return nil }
    return (try? JSONDecoder().decode(AppIconIndex.self, from: data))?
      .checked
  }

  /// Removes direct files without following links or descending into directories.
  ///
  /// - Parameter folder: The verified store directory.
  nonisolated static func empty(_ folder: IconCacheDirectory) {
    for name in folder.names { folder.remove(name) }
  }

  /// Removes files not named by the checked index, relative to the verified handle.
  ///
  /// - Parameters:
  ///   - folder: The verified store directory.
  ///   - kept: The icon names to retain.
  nonisolated static func sweep(_ folder: IconCacheDirectory, keeping kept: Set<String>) {
    for name in folder.names where name != indexName && !kept.contains(name) { folder.remove(name) }
  }

  /// An icon's file as a picture, when it is the size the index says and decodes within
  /// ``AppIconCachePolicy/largestSide``.
  ///
  /// - Parameters:
  ///   - file: The icon's file name.
  ///   - folder: The verified store directory.
  ///   - bytes: Its size, as the index says.
  /// - Returns: The picture, or nil.
  nonisolated static func read(_ file: String, in folder: IconCacheDirectory, bytes: Int) -> CGImage? {
    guard let data = folder.read(file, largest: AppIconCachePolicy.largestFile), data.count == bytes else {
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
