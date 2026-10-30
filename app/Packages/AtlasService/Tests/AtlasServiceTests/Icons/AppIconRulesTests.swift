import Foundation
import Testing

@testable import AtlasService

@Suite("App icon rules")
struct AppIconRulesTests {
  /// The person's home in these tests.
  static let home = "/Users/person"

  @Test("an icon is read only from where apps are installed")
  func trustedRoots() {
    let trusted = [
      "/Applications/Opie.app",
      "/System/Applications/Notes.app",
      "/System/Applications/Utilities/Terminal.app",
      "/System/Library/CoreServices/Finder.app",
      "/Users/person/Applications/Probe.app",
      "/Applications//Safari.app",
      "/System/Cryptexes/App/System/Applications/Safari.app",
    ]
    for path in trusted {
      #expect(TrustedAppRoots.contains(path, home: Self.home), "\(path)")
    }
    let refused = [
      "/System/Library/Evil.app",
      "/Users/Shared/Evil.app",
      "/tmp/Evil.app",
      "/Applications/../tmp/Evil.app",
      "Applications/Opie.app",
      "/ApplicationsEvil/Opie.app",
      "/Users/atlas-helper/Applications/Probe.app",
      "/System/Volumes/Data/Users/Shared/Evil.app",
      "/System/Cryptexes/Evil.app",
      "",
    ]
    for path in refused {
      #expect(!TrustedAppRoots.contains(path, home: Self.home), "\(path)")
    }
  }

  @Test("an app is looked for by its name only as a bundle of that name in a folder")
  func bundleNamed() {
    #expect(TrustedAppRoots.bundle(named: "Notes", in: "/System/Applications") == "/System/Applications/Notes.app")
    #expect(TrustedAppRoots.bundle(named: "../Evil", in: "/Applications") == nil)
    #expect(TrustedAppRoots.bundle(named: "a/b", in: "/Applications") == nil)
    #expect(TrustedAppRoots.folders(home: Self.home).last == "/Users/person/Applications")
  }

  @Test("a bundle id with a slash or a space is no key")
  func bundleKeys() {
    #expect(AppIconKey(bundleId: "com.apple.TextEdit") == .bundle("com.apple.TextEdit"))
    #expect(AppIconKey(bundleId: "test.atlas.install-probe") == .bundle("test.atlas.install-probe"))
    let refused = [
      "com.apple/TextEdit", "com apple", "", "com..apple", ".com", "com.", "com_apple",
      String(repeating: "a", count: AppIconKey.longestBundleId + 1),
    ]
    for text in refused {
      #expect(AppIconKey(bundleId: text) == nil, "\(text)")
    }
    #expect(AppIconKey(bundleId: nil) == nil)
  }

  @Test("a name is a key in any case, and never when it is a path or was cut short")
  func nameKeys() {
    #expect(AppIconKey(name: " Notes ") == .name("notes"))
    #expect(AppIconKey(name: "NOTES") == AppIconKey(name: "notes"))
    #expect(AppIconKey(name: "Cafe\u{301}") == AppIconKey(name: "Café"))
    for text in ["", "  ", "/tmp/Probe", "..", "Probe…", String(repeating: "a", count: AppIconKey.longestName + 1)] {
      #expect(AppIconKey(name: text) == nil, "\(text)")
    }
    #expect(AppIconKey(name: "Notes")?.text != AppIconKey(bundleId: "notes")?.text)
  }

  @Test("past its limits the cache drops the least recently used icons")
  func eviction() {
    var index = AppIconIndex()
    for number in 0...AppIconCachePolicy.mostIcons {
      index.icons["test.app\(number)"] = entry(used: number, bytes: 1)
    }
    let dropped = AppIconCachePolicy.evicted(index)
    #expect(dropped.count == index.icons.count - AppIconCachePolicy.keptIcons)
    #expect(Set(dropped) == Set((0..<dropped.count).map { "test.app\($0)" }))

    var heavy = AppIconIndex()
    let large = AppIconCachePolicy.largestFile
    for number in 0..<(AppIconCachePolicy.mostBytes / large + 1) {
      heavy.icons["test.heavy\(number)"] = entry(used: 100 - number, bytes: large)
    }
    let shed = AppIconCachePolicy.evicted(heavy)
    let left = heavy.icons.filter { !shed.contains($0.key) }
    #expect(left.values.map(\.bytes).reduce(0, +) <= AppIconCachePolicy.keptBytes)
    #expect(shed.allSatisfy { id in left.values.allSatisfy { $0.used >= heavy.icons[id]?.used ?? 0 } })

    index.icons = Dictionary(uniqueKeysWithValues: index.icons.prefix(AppIconCachePolicy.mostIcons).map { $0 })
    #expect(AppIconCachePolicy.evicted(index).isEmpty, "Within its limits nothing goes")
  }

  @Test("an icon not seen for thirty days is looked up again")
  func lookAgain() {
    let read = entry(used: 100, bytes: 1)
    #expect(!AppIconCachePolicy.looksAgain(read, today: 100 + AppIconCachePolicy.lookAgainDays - 1))
    #expect(AppIconCachePolicy.looksAgain(read, today: 100 + AppIconCachePolicy.lookAgainDays))
  }

  @Test("corrupt persisted days are refused and age arithmetic handles both integer boundaries")
  func corruptDaysCannotOverflow() {
    let valid = entry(used: 20_000, bytes: 1)
    for day in [Int.min, -1, Int.max] {
      for field in [\AppIconIndex.Entry.read, \.used] {
        var corrupt = valid
        corrupt[keyPath: field] = day
        #expect(AppIconIndex(icons: ["test.app": corrupt]).checked?.icons.isEmpty == true)
      }
    }
    var old = valid
    old.read = Int.min
    #expect(AppIconCachePolicy.looksAgain(old, today: Int.max))
    #expect(!AppIconCachePolicy.looksAgain(old, today: Int.min))
    old.read = Int.max
    #expect(!AppIconCachePolicy.looksAgain(old, today: Int.min))
    #expect(!AppIconCachePolicy.looksAgain(old, today: Int.max))
    #expect(AppIconCachePolicy.accepts(day: 0))
    #expect(AppIconCachePolicy.accepts(day: AppIconCachePolicy.day(of: .distantFuture)))
  }

  @Test("the cache keeps no time of use finer than a day")
  func days() {
    let morning = Date(timeIntervalSince1970: 20_000 * 86_400 + 60)
    let night = Date(timeIntervalSince1970: 20_000 * 86_400 + 86_000)
    #expect(AppIconCachePolicy.day(of: morning) == 20_000)
    #expect(AppIconCachePolicy.day(of: night) == 20_000)
  }

  @Test("an icon too large to keep is refused")
  func limits() {
    #expect(AppIconCachePolicy.accepts(bytes: AppIconCachePolicy.largestFile))
    #expect(!AppIconCachePolicy.accepts(bytes: AppIconCachePolicy.largestFile + 1))
    #expect(!AppIconCachePolicy.accepts(bytes: 0))
    #expect(AppIconCachePolicy.accepts(width: 128, height: 128))
    #expect(!AppIconCachePolicy.accepts(width: AppIconCachePolicy.largestSide + 1, height: 128))
  }

  @Test("an index keeps only icons under bundle ids in its own folder, and another shape starts over")
  func checkedIndex() {
    let good = entry(used: 1, bytes: 10)
    var bad = good
    bad.file = "../../evil.png"
    let index = AppIconIndex(
      icons: ["com.apple.Notes": good, "not an id": good, "com.apple.Evil": bad],
      names: ["notes": "com.apple.Notes", "Notes": "com.apple.Notes", "evil": "a/b"]
    )
    let checked = index.checked
    #expect(checked?.icons.keys.sorted() == ["com.apple.Notes"])
    #expect(checked?.names == ["notes": "com.apple.Notes"])
    #expect(AppIconIndex(version: 2).checked == nil)
  }

  @Test("past their limit the names kept for no icon are forgotten first")
  func names() {
    var index = AppIconIndex()
    index.icons["test.kept"] = entry(used: 1, bytes: 1)
    for number in 0...AppIconCachePolicy.mostIcons {
      index.names["app \(number)"] = "test.gone\(number)"
    }
    index.names["kept"] = "test.kept"
    let forgotten = AppIconCachePolicy.forgottenNames(index)
    #expect(forgotten.count == index.names.count - AppIconCachePolicy.keptIcons)
    #expect(!forgotten.contains("kept"))
  }

  /// An entry used and read on a day.
  ///
  /// - Parameters:
  ///   - used: The day it was used and read.
  ///   - bytes: Its size.
  /// - Returns: The entry.
  private func entry(used: Int, bytes: Int) -> AppIconIndex.Entry {
    AppIconIndex.Entry(file: "0123456789abcdef.png", name: nil, used: used, read: used, bytes: bytes, sha: "00")
  }
}
