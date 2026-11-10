import Foundation
import Testing

@testable import AtlasApp

@Suite("SiteIconLinks")
struct SiteIconLinksTests {
  // swift-format-ignore: NeverForceUnwrap
  // A literal address always parses.
  let page = URL(string: "https://github.com/")!

  @Test("Apple's touch icon comes first, then the others largest first, then the site's favicon.ico")
  func theBestIconComesFirst() {
    let html = """
      <head><link rel="icon" href="/small.png" sizes="16x16">
      <LINK REL='shortcut icon' HREF='https://cdn.example/big.png' sizes='16x16 64x64'>
      <link rel="apple-touch-icon" href="touch.png"><link rel="stylesheet" href="/site.css"></head>
      """
    #expect(
      SiteIconLinks.candidates(html: html, page: page).map(\.absoluteString) == [
        "https://github.com/touch.png", "https://cdn.example/big.png", "https://github.com/small.png",
        "https://github.com/favicon.ico",
      ]
    )
  }

  @Test("an SVG icon, one with no address and one that is no web address are left out, and none is named twice")
  func unusableIconsAreLeftOut() {
    let html = """
      <link rel="icon" href="/mark.svg"><link rel="mask-icon" type="image/svg+xml" href="/pin">
      <link rel="icon" href=""><link rel="icon" href="javascript:alert(1)"><link rel="icon" href="/favicon.ico">
      """
    #expect(
      SiteIconLinks.candidates(html: html, page: page).map(\.absoluteString) == ["https://github.com/favicon.ico"]
    )
  }

  @Test("an older touch icon counts as Apple's, and a pinned tab's mask or a stylesheet is no icon")
  func relNamesAreMatchedWhole() {
    let html = """
      <link rel="apple-touch-icon-precomposed" href="/old.png"><link rel="mask-icon" href="/pin.png">
      <link rel="iconography" href="/x.png">
      """
    #expect(
      SiteIconLinks.candidates(html: html, page: page).map(\.absoluteString) == [
        "https://github.com/old.png", "https://github.com/favicon.ico",
      ]
    )
  }

  @Test("a page with no icon links still offers the site's favicon.ico")
  func aPageWithoutLinksFallsBack() {
    #expect(
      SiteIconLinks.candidates(html: "<html></html>", page: page).map(\.absoluteString) == [
        "https://github.com/favicon.ico"
      ]
    )
  }

  @Test("sizes names the largest side, and any or nothing counts as none")
  func sizesNameTheLargestSide() {
    #expect(SiteIconLinks.size("16x16 32x32") == 32)
    #expect(SiteIconLinks.size("any") == 0)
    #expect(SiteIconLinks.size(nil) == 0)
  }
}
