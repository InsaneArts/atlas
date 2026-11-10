import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// A site's own icon, as a listed account's tile shows it: read from the site itself, never from a service that would
/// learn which sites the helper is signed in to, over a session that keeps no cookies and sends no sign-in. The page's
/// start says where its icon is (``SiteIconLinks``); the first that reads as a picture is kept on disk, so the site is
/// asked again only once that copy is ``fresh`` no longer. A site that can't be reached keeps the icon it had, or has
/// none and its first letter shows.
nonisolated struct SiteIcons: Sendable {
  /// How long a kept icon is used before the site is asked again: 30 days.
  static let fresh: TimeInterval = 30 * 24 * 60 * 60
  /// The most of a page read for its links: 512 KB, more than any page's head.
  static let pageLimit = 512 * 1024
  /// The most of an icon read: 1 MB.
  static let iconLimit = 1024 * 1024
  /// The smallest side an icon may have to be kept: 16 pixels.
  static let smallest = 16

  /// Reads at most `limit` bytes from an address, returning them and where they came from after any redirect.
  var read: @Sendable (_ url: URL, _ limit: Int) async throws -> (Data, URL)
  /// Where icons are kept, or nil to keep none.
  var folder: URL?
  /// The time now.
  var now: @Sendable () -> Date = { Date() }

  /// The person's own: read over one ephemeral session, kept in their caches.
  static let live: SiteIcons = {
    let configuration = URLSessionConfiguration.ephemeral
    configuration.httpCookieAcceptPolicy = .never
    configuration.httpShouldSetCookies = false
    configuration.urlCache = nil
    configuration.timeoutIntervalForRequest = 5
    configuration.timeoutIntervalForResource = 10
    let session = URLSession(configuration: configuration)
    let caches = FileManager.default
      .urls(for: .cachesDirectory, in: .userDomainMask)
      .first
    return SiteIcons(
      read: { url, limit in
        let (bytes, response) = try await session.bytes(from: url)
        guard (response as? HTTPURLResponse).map({ (200..<300).contains($0.statusCode) }) ?? false else {
          throw URLError(.badServerResponse)
        }
        var data = Data()
        for try await byte in bytes {
          data.append(byte)
          if data.count >= limit { break }
        }
        return (data, response.url ?? url)
      },
      folder: caches?.appendingPathComponent("Atlas/SiteIcons", isDirectory: true)
    )
  }()

  /// The icon of `site`, a domain such as `github.com`; nil when it has none that can be read.
  ///
  /// - Parameter site: The site.
  /// - Returns: Its icon.
  func icon(for site: String) async -> CGImage? {
    guard let page = Self.address(of: site) else { return nil }
    let kept = folder?.appendingPathComponent("\(site).png")
    if let kept, let image = Self.keptIcon(at: kept, now: now(), fresh: true) { return image }
    var candidates = [URL(string: "/favicon.ico", relativeTo: page)?.absoluteURL].compactMap { $0 }
    if let (data, landed) = try? await read(page, Self.pageLimit) {
      candidates = SiteIconLinks.candidates(html: String(decoding: data, as: UTF8.self), page: landed)
    }
    for candidate in candidates {
      guard let (data, _) = try? await read(candidate, Self.iconLimit), let image = Self.largest(in: data) else {
        continue
      }
      if let kept { Self.keep(image, at: kept) }
      return image
    }
    // A site that can't be reached now keeps the icon it had.
    return kept.flatMap { Self.keptIcon(at: $0, now: now(), fresh: false) }
  }

  /// The address of `site`'s own page, or nil when `site` is no plain domain.
  ///
  /// - Parameter site: The site.
  /// - Returns: `https://site/`.
  static func address(of site: String) -> URL? {
    guard !site.isEmpty, site.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "." || $0 == "-" }) else { return nil }
    var components = URLComponents()
    components.scheme = "https"
    components.host = site
    components.path = "/"
    return components.url
  }

  /// The largest picture in `data`, as an `.ico` holds several; nil when none is at least ``smallest`` across.
  ///
  /// - Parameter data: The icon's bytes.
  /// - Returns: The picture.
  static func largest(in data: Data) -> CGImage? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
    return (0..<CGImageSourceGetCount(source))
      .compactMap { CGImageSourceCreateImageAtIndex(source, $0, nil) }
      .filter { min($0.width, $0.height) >= smallest }
      .max { $0.width < $1.width }
  }

  /// The icon kept at `url`.
  ///
  /// - Parameters:
  ///   - url: Where it is kept.
  ///   - now: The time now.
  ///   - fresh: Whether only one kept for less than ``SiteIcons/fresh`` will do.
  /// - Returns: The icon, or nil when none is kept, or it is stale and only a fresh one would do.
  static func keptIcon(at url: URL, now: Date, fresh: Bool) -> CGImage? {
    guard let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
      !fresh || now.timeIntervalSince(modified) < Self.fresh,
      let data = try? Data(contentsOf: url)
    else { return nil }
    return largest(in: data)
  }

  /// Keeps `image` at `url` as a PNG; a failure keeps nothing, and the site is asked again next time.
  ///
  /// - Parameters:
  ///   - image: The icon.
  ///   - url: Where to keep it.
  static func keep(_ image: CGImage, at url: URL) {
    let folder = url.deletingLastPathComponent()
    try? FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    let data = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else {
      return
    }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else { return }
    let png = data as Data
    try? png.write(to: url, options: .atomic)
  }
}
