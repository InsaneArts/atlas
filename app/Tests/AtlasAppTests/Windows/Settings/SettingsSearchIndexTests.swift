import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Settings search index")
struct SettingsSearchIndexTests {
  @Test("search folds case, diacritics and repeated whitespace and treats whitespace alone as empty")
  func searchNormalizesWords() {
    let index = SettingsSearchIndex(entries: [SettingsSearchEntry(id: .startup, title: "Café at login")])
    #expect(index.results(for: "  CAFE\n AT\tlogin  ").map(\.id) == [.startup])
    #expect(index.results(for: " \n\t ").isEmpty)
    #expect(SettingsSearchIndex.normalize(" É  A\nB ") == "e a b")
  }

  @Test("every query term must match across the title, section, description or aliases")
  func everyTermMustMatch() {
    let entry = SettingsSearchEntry(
      id: .cadence,
      title: "Screen updates",
      detail: "How often",
      section: "Viewer",
      aliases: ["Live", "5 seconds"]
    )
    let index = SettingsSearchIndex(entries: [entry])
    #expect(index.results(for: "live screen viewer often") == [entry])
    #expect(index.results(for: "screen missing").isEmpty)
    #expect(index.results(for: "violet telescope").isEmpty)
  }

  @Test("exact titles precede title prefixes, title terms and description or alias matches")
  func titleMatchesRankFirst() {
    let entries = [
      SettingsSearchEntry(id: .account, title: "Account", aliases: ["screen"]),
      SettingsSearchEntry(id: .cadence, title: "Update screen"),
      SettingsSearchEntry(id: .startup, title: "Screen on login"),
      SettingsSearchEntry(id: .notifications, title: "Screen"),
    ]
    let index = SettingsSearchIndex(entries: entries)
    #expect(index.results(for: "screen").map(\.id) == [.notifications, .startup, .cadence, .account])
  }

  @Test("equal ranks use stable page order followed by presentation order")
  func tiesStayStable() {
    let entries = [
      SettingsSearchEntry(id: .setup, title: "Second", aliases: ["match"]),
      SettingsSearchEntry(id: .logOut, title: "First helper", aliases: ["match"]),
      SettingsSearchEntry(id: .recover, title: "Second helper", aliases: ["match"]),
      SettingsSearchEntry(id: .startup, title: "General", aliases: ["match"]),
    ]
    let index = SettingsSearchIndex(entries: entries)
    let expected: [SettingsSearchTarget] = [.logOut, .recover, .startup, .setup]
    for _ in 0..<3 { #expect(index.results(for: "match").map(\.id) == expected) }
  }

  @Test("multiple aliases and duplicate metadata resolve to one stable control target")
  func aliasesDoNotDuplicateControls() {
    let index = SettingsSearchIndex(entries: [
      SettingsSearchEntry(id: .cadence, title: "Screen updates", aliases: ["Live", "5 seconds", "Paused"]),
      SettingsSearchEntry(id: .cadence, title: "Screen updates", aliases: ["pause"]),
    ])
    for query in ["live", "5 seconds", "paused", "pause", "live pause"] {
      #expect(index.results(for: query).map(\.id) == [.cadence])
    }
  }

  @Test("an optimized index filters a thousand loaded projects without stale or duplicate results")
  func aThousandProjectsRemainLocal() {
    let entries = (0..<1_000)
      .map { number in
        SettingsSearchEntry(
          id: .project("/sample/work/\(number)"),
          title: "Project \(number)",
          detail: "~/work/\(number)",
          aliases: ["Forget"]
        )
      }
    let metadata = SettingsSearchCatalog.entries(SettingsPresenter.state(SettingsSnapshot()))
    let index = SettingsSearchIndex(entries: entries + metadata)
    let clock = ContinuousClock()
    var matches: [SettingsSearchEntry] = []
    let elapsed = clock.measure {
      for _ in 0..<100 { matches = index.results(for: "project work") }
    }
    #expect(matches.count == 1_000)
    #expect(Set(matches.map(\.id)).count == 1_000)
    print(
      "Settings search: 100 filters / 1,000 projects: \(elapsed); \(ProcessInfo.processInfo.operatingSystemVersionString)"
    )
  }
}
