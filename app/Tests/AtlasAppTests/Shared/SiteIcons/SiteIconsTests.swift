import Foundation
import Testing

@testable import AtlasApp

@Suite("SiteIcons")
struct SiteIconsTests {
  /// Reads answered from `pages`, by address, noting each address asked for.
  nonisolated final class Pages: @unchecked Sendable {
    let lock = NSLock()
    var answers: [String: Data]
    var asked: [String] = []

    init(_ answers: [String: Data]) { self.answers = answers }

    func read(_ url: URL) throws -> (Data, URL) {
      lock.lock()
      defer { lock.unlock() }
      asked.append(url.absoluteString)
      guard let data = answers[url.absoluteString] else { throw URLError(.fileDoesNotExist) }
      return (data, url)
    }
  }

  /// A folder of its own for one test's kept icons.
  static func scratch() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString)
  }

  /// Removes a test's folder.
  static func remove(_ folder: URL) {
    try? FileManager.default
      .removeItem(at: folder)
  }

  @Test("the icon the page links to is read and kept, and a fresh kept one is used without asking the site again")
  func theLinkedIconIsReadAndKept() async throws {
    let folder = Self.scratch()
    defer { Self.remove(folder) }
    let pages = Pages([
      "https://github.com/": Data(#"<link rel="apple-touch-icon" href="/touch.png">"#.utf8),
      "https://github.com/touch.png": png(width: 64, height: 64),
    ])
    let icons = SiteIcons(read: { url, _ in try pages.read(url) }, folder: folder)
    let icon = try #require(await icons.icon(for: "github.com"))
    #expect(icon.width == 64)
    #expect(pages.asked == ["https://github.com/", "https://github.com/touch.png"])
    #expect(await icons.icon(for: "github.com")?.width == 64)
    #expect(pages.asked.count == 2, "the kept icon answered")
  }

  @Test("a stale icon is asked for again, and kept on when the site can't be reached")
  func aStaleIconStaysWhenTheSiteIsGone() async throws {
    let folder = Self.scratch()
    defer { Self.remove(folder) }
    try FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    let kept = folder.appendingPathComponent("github.com.png")
    let picture = png(width: 48, height: 48)
    try picture.write(to: kept)
    let old = Date(timeIntervalSinceNow: -SiteIcons.fresh - 60)
    try FileManager.default
      .setAttributes([.modificationDate: old], ofItemAtPath: kept.path)
    let pages = Pages([:])
    let icons = SiteIcons(read: { url, _ in try pages.read(url) }, folder: folder)
    #expect(await icons.icon(for: "github.com")?.width == 48)
    #expect(pages.asked.contains("https://github.com/"), "the site was asked again")
  }

  @Test("a page that can't be read falls back to favicon.ico, and a site with neither has no icon")
  func anUnreadablePageFallsBack() async {
    let pages = Pages(["https://acme.dev/favicon.ico": png(width: 32, height: 32)])
    let icons = SiteIcons(read: { url, _ in try pages.read(url) }, folder: nil)
    #expect(await icons.icon(for: "acme.dev")?.width == 32)
    #expect(await icons.icon(for: "gone.example") == nil)
  }

  @Test("an icon smaller than 16 pixels is not kept, and only a plain domain is ever asked")
  func tinyIconsAndOddSitesAreRefused() async {
    let pages = Pages(["https://acme.dev/favicon.ico": png(width: 8, height: 8)])
    let icons = SiteIcons(read: { url, _ in try pages.read(url) }, folder: nil)
    #expect(await icons.icon(for: "acme.dev") == nil)
    #expect(SiteIcons.address(of: "github.com")?.absoluteString == "https://github.com/")
    for odd in ["", "evil.com/path", "user@host", "a b"] {
      #expect(SiteIcons.address(of: odd) == nil, "\(odd)")
    }
  }
}
