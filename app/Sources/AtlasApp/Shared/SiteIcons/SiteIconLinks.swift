import Foundation

/// Where a site's own page says its icon is: the `<link>` tags whose `rel` names an icon, best first, then the
/// `/favicon.ico` every browser falls back to. Apple's touch icon comes first, as it is drawn large for a tile, then a
/// large `fluid-icon` such as GitHub's, then the largest of the others by their `sizes`. An SVG icon
/// is left out, as it can't be drawn as a picture here.
nonisolated enum SiteIconLinks {
  /// The icons `html`, read from `page`, links to, best first, ending with the site's `/favicon.ico`.
  ///
  /// - Parameters:
  ///   - html: The page's HTML, or its start.
  ///   - page: Where the page was read from, after any redirect, which relative links resolve against.
  /// - Returns: The icons' addresses, each once.
  static func candidates(html: String, page: URL) -> [URL] {
    let ranked = links(in: html)
      .compactMap { icon($0, page: page) }
      .sorted { $0.rank > $1.rank }
      .map(\.url)
    let fallback = URL(string: "/favicon.ico", relativeTo: page)?.absoluteURL
    var seen = Set<URL>()
    var each: [URL] = []
    for url in ranked + [fallback].compactMap({ $0 }) where seen.insert(url).inserted {
      each.append(url)
    }
    return each
  }

  /// The icon one `<link>` tag names, with how good it is: Apple's touch icon best, then by its largest side; nil for
  /// a tag that names no icon, or one that is no web address or is an SVG.
  ///
  /// - Parameters:
  ///   - link: The tag's attributes.
  ///   - page: The page the tag is on.
  /// - Returns: The icon's address and rank.
  static func icon(_ link: [String: String], page: URL) -> (url: URL, rank: Int)? {
    let rel = link["rel", default: ""]
      .lowercased()
      .split(separator: " ")
    let touch = rel.contains { $0.hasPrefix("apple-touch-icon") }
    let fluid = rel.contains("fluid-icon")
    let href = link["href", default: ""]
      .replacingOccurrences(of: "&amp;", with: "&")
    guard touch || fluid || rel.contains("icon"), !href.isEmpty,
      let url = URL(string: href, relativeTo: page)?.absoluteURL,
      url.scheme == "https" || url.scheme == "http",
      !url.path.lowercased().hasSuffix(".svg"), link["type"]?.lowercased() != "image/svg+xml"
    else { return nil }
    return (url, touch ? Int.max : fluid ? Int.max - 1 : size(link["sizes"]))
  }

  /// Each `<link>` tag's attributes in `html`, by their lower-cased names.
  ///
  /// - Parameter html: The HTML.
  /// - Returns: The tags' attributes.
  static func links(in html: String) -> [[String: String]] {
    let tag = /<link\b[^>]*>/.ignoresCase()
    let attribute = /([a-zA-Z-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))/
    return html.matches(of: tag)
      .map { match in
        var attributes: [String: String] = [:]
        for found in match.output.matches(of: attribute) {
          let value = found.output.2 ?? found.output.3 ?? found.output.4 ?? ""
          attributes[String(found.output.1).lowercased()] = String(value)
        }
        return attributes
      }
  }

  /// The largest side `sizes` names, such as 32 for `16x16 32x32`; 0 for none or `any`.
  ///
  /// - Parameter sizes: The `sizes` attribute.
  /// - Returns: The side.
  static func size(_ sizes: String?) -> Int {
    (sizes ?? "")
      .lowercased()
      .split(separator: " ")
      .compactMap { side in
        side
          .split(separator: "x")
          .first
          .flatMap { Int($0) }
      }
      .max() ?? 0
  }
}
