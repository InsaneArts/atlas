import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerStatus.Account")
struct AccountDecodingTests {
  @Test("a listed account reads whole, and one that doesn't read is dropped")
  func aListedAccountReadsWhole() throws {
    let reading = try status([
      "overview": [
        "accounts": [
          [
            "kind": "site", "id": "github.com", "name": "github.com", "browser": "com.google.Chrome",
            "added": 1_762_600_000_000, "how": "takeControl", "level": "ask",
          ],
          ["kind": "app", "id": "com.tinyspeck.slackmacgap", "name": "Slack", "added": 1, "how": "byHand"],
          ["kind": "planet", "id": "x", "name": "x", "added": 1],
        ] as [Any],
        "accountNote": ["name": "github.com", "added": 1_762_600_000_000],
      ]
    ])
    let accounts = try #require(reading.overview.accounts)
    #expect(accounts.map(\.name) == ["github.com", "Slack"])
    #expect(accounts.first?.addedAfterPassword == true)
    #expect(accounts.first?.browser == "com.google.Chrome")
    #expect(accounts.last?.addedAfterPassword == false)
    #expect(reading.overview.accountNote?.name == "github.com")
  }

  @Test("a list the service can't read, or doesn't send, is unknown, never empty")
  func anUnreadListIsUnknown() throws {
    #expect(try status(["overview": ["accounts": NSNull()]]).overview.accounts == nil)
    #expect(try status(["overview": [:] as [String: Any]]).overview.accounts == nil)
    #expect(try status(["overview": ["accounts": [] as [Any]]]).overview.accounts == [])
    #expect(try status(["overview": ["accounts": [:] as [String: Any]]]).overview.accounts == nil, "not a list")
    #expect(try status(["overview": ["accounts": ["x", 7] as [Any]]]).overview.accounts == nil, "nothing reads")
  }
}
