import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityLog.Account and Allowance, on a row")
struct ActivityAccountDecodingTests {
  /// A site account in Chrome, as the service writes it.
  static let site = ActivityLog.Account(
    name: "github.com",
    key: AccountKey(kind: .site, id: "github.com", browser: "com.google.Chrome")
  )

  @Test("an activity row reads the account it acted in and what let it act there, for each allowance")
  func aRowReadsItsAccount() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "account": {"name": "github.com", "key": {"kind": "site", "id": "github.com",
          "browser": "com.google.Chrome"}}, "allowed": "task"},
        {"id": "2", "account": {"name": "Slack", "key": {"kind": "app", "id": "com.tinyspeck.slackmacgap"}},
          "allowed": "once"},
        {"id": "3", "account": {"name": "github.com", "key": {"kind": "site", "id": "github.com",
          "browser": "com.google.Chrome"}}, "allowed": "trusted"},
        {"id": "4", "account": {"name": "github.com", "key": {"kind": "site", "id": "github.com",
          "browser": "com.google.Chrome"}}}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.account?.name) == ["github.com", "Slack", "github.com", "github.com"])
    #expect(entries.first?.account == Self.site)
    #expect(entries[1].account?.key == AccountKey(kind: .app, id: "com.tinyspeck.slackmacgap"))
    #expect(entries.map(\.allowed) == [.task, .once, .trusted, nil])
  }

  @Test("a row from an older service, or with an account that doesn't read, keeps the row and names no account")
  func anOddAccountIsNone() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "says": "clicked Save"},
        {"id": "2", "account": null, "allowed": null},
        {"id": "3", "account": "github.com", "allowed": "task"},
        {"id": "4", "account": {"name": "", "key": {"kind": "site", "id": "github.com"}}},
        {"id": "5", "account": {"name": "github.com", "key": {"kind": "planet", "id": "x"}}},
        {"id": "6", "account": {"name": "github.com"}, "allowed": "once"},
        {"id": "7", "account": {"name": "github.com", "key": {"kind": "site", "id": ""}}},
        {"id": "8", "allowed": "trusted"}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.id) == ["1", "2", "3", "4", "5", "6", "7", "8"], "every row is kept")
    #expect(entries.allSatisfy { $0.account == nil })
    #expect(entries.allSatisfy { $0.allowed == nil }, "what let it act counts only with an account")
  }

  @Test("an allowance of the wrong type or a word this app doesn't know is none, and the account stays")
  func anOddAllowanceIsNone() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "account": {"name": "github.com", "key": {"kind": "site", "id": "github.com"}}, "allowed": 3},
        {"id": "2", "account": {"name": "github.com", "key": {"kind": "site", "id": "github.com"}},
          "allowed": "forever"},
        {"id": "3", "account": {"name": "github.com", "key": {"kind": "site", "id": "github.com", "browser": 7}},
          "allowed": "once"}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.account?.name) == ["github.com", "github.com", "github.com"])
    #expect(entries.map(\.allowed) == [nil, nil, .once])
    #expect(entries.last?.account?.key.browser == nil, "a browser of the wrong type is none")
  }

  @Test("the overview's activity reads the account and what let it act there, and an odd one is none")
  func theOverviewReadsItsAccount() throws {
    let account: [String: Any] = [
      "name": "github.com", "key": ["kind": "site", "id": "github.com", "browser": "com.google.Chrome"],
    ]
    let reading = try status([
      "overview": [
        "activity": [
          ["operation": "input", "ok": true, "account": account, "allowed": "task"],
          ["operation": "input", "ok": true, "account": account, "allowed": "trusted"],
          ["operation": "input", "ok": true, "account": ["name": 4], "allowed": "once"],
          ["operation": "input", "ok": true, "account": account, "allowed": false],
          ["operation": "input", "ok": true],
        ] as [Any]
      ]
    ])
    let rows = reading.overview.activity
    #expect(rows.count == 5, "every row is kept")
    #expect(rows.map(\.account) == [Self.site, Self.site, nil, Self.site, nil])
    #expect(rows.map(\.allowed) == [.task, .trusted, nil, nil, nil])
  }
}
