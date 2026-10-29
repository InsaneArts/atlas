import AppKit
import AtlasService
import ImageIO
import UniformTypeIdentifiers

/// Where an app's icon is read on the person's side: from an app's bundle, the bundle the workspace knows for a
/// bundle identifier, or the bundle of an app's name in the folders apps are installed in. Every read is under
/// ``TrustedAppRoots``, so no icon comes from a folder another app could have written, and every icon is drawn at
/// ``AppIconCachePolicy/side`` pixels as a PNG. The reads run off the main actor; a test passes its own.
nonisolated struct AppIconLookup: Sendable {
  /// The PNG of the icon of an app's bundle, given its path; nil when it is not an app installed where apps go.
  var icon: @Sendable (_ bundle: String) async -> Data?
  /// Where the app of a bundle identifier is installed; nil when the workspace knows none, or one elsewhere.
  var bundleOfId: @Sendable (_ bundleId: String) async -> String?
  /// Where an app of a name is installed, as `<Name>.app` in the folders apps go in; nil when none is.
  var bundleNamed: @Sendable (_ name: String) async -> String?
  /// The bundle identifier of an app's bundle, given its path.
  var bundleId: @Sendable (_ bundle: String) async -> String?

  /// No reads at all: every icon stays its drawn tile.
  static let none = AppIconLookup(
    icon: { _ in nil },
    bundleOfId: { _ in nil },
    bundleNamed: { _ in nil },
    bundleId: { _ in nil }
  )

  /// The reads through the workspace, under the person's home.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The reads.
  static func workspace(home: String) -> AppIconLookup {
    AppIconLookup(
      icon: { bundle in await png(atBundle: bundle, home: home) },
      bundleOfId: { id in await bundle(ofId: id, home: home) },
      bundleNamed: { name in await bundle(named: name, home: home) },
      bundleId: { bundle in await identifier(atBundle: bundle, home: home) }
    )
  }

  /// The icon of an app installed where apps go, as the Finder draws it, at ``AppIconCachePolicy/side`` pixels.
  ///
  /// - Parameters:
  ///   - path: The app's bundle.
  ///   - home: The person's home folder.
  /// - Returns: The icon as a PNG, or nil.
  @concurrent nonisolated static func png(atBundle path: String, home: String) async -> Data? {
    guard let bundle = drawnBundle(path, home: home) else { return nil }
    return autoreleasepool {
      let side = AppIconCachePolicy.side
      var proposed = CGRect(x: 0, y: 0, width: side, height: side)
      guard
        let drawn = NSWorkspace.shared
          .icon(forFile: bundle)
          .cgImage(forProposedRect: &proposed, context: nil, hints: nil),
        let space = CGColorSpace(name: CGColorSpace.sRGB),
        let context = CGContext(
          data: nil,
          width: side,
          height: side,
          bitsPerComponent: 8,
          bytesPerRow: 0,
          space: space,
          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
      else { return nil }
      context.interpolationQuality = .high
      context.draw(drawn, in: CGRect(x: 0, y: 0, width: side, height: side))
      return context.makeImage()
        .flatMap(encoded)
    }
  }

  /// Where the workspace says the app of a bundle identifier is, when that is where apps are installed; an answer
  /// anywhere else is refused.
  ///
  /// - Parameters:
  ///   - id: The bundle identifier.
  ///   - home: The person's home folder.
  /// - Returns: The bundle's path, or nil.
  @concurrent nonisolated static func bundle(ofId id: String, home: String) async -> String? {
    guard AppIconKey.isBundleId(id), let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) else {
      return nil
    }
    let path = url.standardizedFileURL.path
    return isInstalledApp(path, home: home) ? path : nil
  }

  /// The first `<Name>.app` in the folders apps are installed in, in order.
  ///
  /// - Parameters:
  ///   - name: The app's name, which must be a key's (``AppIconKey/init(name:)``).
  ///   - home: The person's home folder.
  /// - Returns: The bundle's path, or nil.
  @concurrent nonisolated static func bundle(named name: String, home: String) async -> String? {
    guard AppIconKey(name: name) != nil else { return nil }
    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
    return TrustedAppRoots.folders(home: home)
      .lazy
      .compactMap { TrustedAppRoots.bundle(named: trimmed, in: $0) }
      .first { isInstalledApp($0, home: home) }
  }

  /// The bundle identifier an app's bundle declares, when it is one.
  ///
  /// - Parameters:
  ///   - path: The app's bundle.
  ///   - home: The person's home folder.
  /// - Returns: The identifier, or nil.
  @concurrent nonisolated static func identifier(atBundle path: String, home: String) async -> String? {
    guard isInstalledApp(path, home: home) else { return nil }
    return Bundle(path: path)?
      .bundleIdentifier
      .flatMap { AppIconKey.isBundleId($0) ? $0 : nil }
  }

  /// The bundle an app's icon is drawn from: the bundle itself, or where it leads when it is a link, such as
  /// `/Applications/Safari.app`, so the app's own icon is drawn rather than an alias's. Both must be apps installed
  /// where apps go, and only the one link is followed.
  ///
  /// - Parameters:
  ///   - path: The app's bundle.
  ///   - home: The person's home folder.
  /// - Returns: The bundle to draw, or nil.
  nonisolated static func drawnBundle(_ path: String, home: String) -> String? {
    guard isInstalledApp(path, home: home) else { return nil }
    guard let link = try? FileManager.default.destinationOfSymbolicLink(atPath: path) else { return path }
    let folder = URL(fileURLWithPath: path)
      .deletingLastPathComponent()
    let target = URL(fileURLWithPath: link, relativeTo: folder)
      .standardizedFileURL
      .path
    return isInstalledApp(target, home: home) ? target : nil
  }

  /// Whether a path is an app's bundle installed where apps go: under ``TrustedAppRoots``, named `.app`, and with
  /// its `Contents` folder, since the workspace draws a generic icon for a bundle that is not there and no icon is
  /// better than a wrong one.
  ///
  /// - Parameters:
  ///   - path: The path.
  ///   - home: The person's home folder.
  /// - Returns: Whether an icon may be read from it.
  nonisolated static func isInstalledApp(_ path: String, home: String) -> Bool {
    guard path.hasSuffix(TrustedAppRoots.bundleSuffix), TrustedAppRoots.contains(path, home: home) else {
      return false
    }
    var isFolder: ObjCBool = false
    let contents = URL(fileURLWithPath: path, isDirectory: true)
      .appendingPathComponent("Contents")
    let files = FileManager.default
    return files.fileExists(atPath: contents.path, isDirectory: &isFolder) && isFolder.boolValue
  }

  /// A picture as PNG data.
  ///
  /// - Parameter image: The picture.
  /// - Returns: Its PNG, or nil should encoding fail.
  nonisolated static func encoded(_ image: CGImage) -> Data? {
    let data = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else {
      return nil
    }
    CGImageDestinationAddImage(destination, image, nil)
    return CGImageDestinationFinalize(destination) ? data as Data : nil
  }
}
