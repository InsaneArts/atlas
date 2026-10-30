import Foundation
import Testing

@testable import AtlasUI

@Suite("AppMonogram")
struct AppMonogramTests {
  @Test("the same name always draws the same tint, in any case")
  func sameTint() {
    // FNV-1a fixes each, so a name draws one colour at every launch and on every Mac.
    let expected: [String: MonogramTint] = [
      "Juniper": .indigo, "Probe": .blue, "Relay": .teal, "Harbor": .green,
      "Ledger": .amber, "Tally": .coral, "Quill": .pink, "Nimbus": .graphite,
    ]
    for (name, tint) in expected {
      #expect(MonogramTint(name: name) == tint, "\(name)")
      #expect(MonogramTint(name: name.uppercased()) == tint, "\(name)")
    }
    #expect(MonogramTint(name: "Cafe\u{301}") == MonogramTint(name: "Café"))
    #expect(MonogramTint.hash("") == MonogramTint.offsetBasis)
  }

  @Test("the letter is the name's first letter or digit, in capitals as the person's language writes them")
  func letters() {
    #expect(AppMonogram(name: "probe").letter == "P")
    #expect(AppMonogram(name: "1Password").letter == "1")
    #expect(AppMonogram(name: "  @atlas").letter == "A")
    #expect(AppMonogram(name: "日本").letter == "日")
    #expect(AppMonogram(name: "…").letter == "…")
    #expect(AppMonogram(name: "").letter == "")
    #expect(AppMonogram(name: "istanbul", locale: Locale(identifier: "tr")).letter == "İ")
  }

  @Test("a face is the icon found, else the tile of the name")
  func faces() {
    #expect(AppIconFace.of(nil, name: "Probe") == .monogram(AppMonogram(name: "Probe")))
  }
}
