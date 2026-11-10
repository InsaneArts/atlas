import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("Ask")
struct AskDecodingTests {
  @Test("a question in asks.json reads whole: who, the account, what it would send and which answers it offers")
  func aQuestionReadsWhole() throws {
    let list = try decoded([
      "asks": [
        [
          "id": "a1", "session": "k1", "who": "Claude Code · release", "task": "post the notes", "account": "Slack",
          "asked": 1_000, "ends": 121_000, "picture": "a1.jpg",
          "question": [
            "what": "send-key", "key": "return", "typed": 0, "once": true, "task": false,
            "frame": ["x": 10, "y": 20, "width": 80, "height": 24], "pictureSize": ["width": 2880, "height": 1800],
            "message": ["digest": "d", "preview": "Atlas 0.1.1 is out", "length": 18],
          ],
        ],
        ["who": "no id"],
      ] as [Any],
      "allowed": [
        [
          "session": "k1", "name": "GitHub", "ends": 3_600_000,
          "account": ["kind": "site", "id": "github.com", "browser": NSNull()],
        ]
      ],
    ])
    let ask = try #require(list.asks.first)
    #expect(list.asks.count == 1, "the one with no id is dropped")
    #expect(ask.kind == .sendKey)
    #expect(ask.message == Ask.Message(preview: "Atlas 0.1.1 is out", length: 18))
    #expect(ask.offersOnce && !ask.offersTask)
    #expect(ask.asked == Date(timeIntervalSince1970: 1) && ask.ends == Date(timeIntervalSince1970: 121))
    #expect(ask.picture == "a1.jpg")
    #expect(ask.frame == Ask.Area(x: 10, y: 20, width: 80, height: 24))
    #expect(ask.pictureSize == ViewerStatus.Size(width: 2880, height: 1800), "the frame's own pixels")
    #expect(list.allowed.map(\.name) == ["GitHub"])
    #expect(list.allowed.first?.account == AccountKey(kind: .site, id: "github.com"))
  }

  @Test("the status's summary has no message and offers Deny alone")
  func theSummaryOffersDenyAlone() throws {
    let reading = try status([
      "overview": ["asks": [["id": "a1", "who": "Codex", "account": "GitHub", "what": "click", "ends": 5_000]]]
    ])
    let ask = try #require(reading.overview.asks.first)
    #expect(ask.kind == .click)
    #expect(ask.message == nil)
    #expect(!ask.offersOnce && !ask.offersTask)
  }

  @Test("a picture that leaves the asks folder is dropped")
  func aPictureOutsideTheFolderIsDropped() throws {
    let list = try decoded([
      "asks": [["id": "a1", "account": "x", "ends": 9_000, "picture": "../secret.jpg"]]
    ])
    let ask = try #require(list.asks.first)
    #expect(ask.ends == Date(timeIntervalSince1970: 9))
    #expect(ask.picture == nil)
    #expect(ask.kind == .unsure, "no question reads as can't tell")
  }

  @Test("an account's key sends its missing browser as null, as the service wrote it")
  func aKeySendsNullBrowser() throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys
    let data = try encoder.encode(AccountKey(kind: .app, id: "com.tinyspeck.slackmacgap"))
    #expect(String(decoding: data, as: UTF8.self) == #"{"browser":null,"id":"com.tinyspeck.slackmacgap","kind":"app"}"#)
  }

  @Test("an account's level reads, and one Atlas doesn't know reads as Ask Before Sending")
  func anAccountsLevelReads() throws {
    let reading = try status([
      "overview": [
        "accounts": [
          ["kind": "site", "id": "bank.com", "name": "bank.com", "added": 1, "level": "lookOnly"],
          ["kind": "app", "id": "x", "name": "X", "added": 1, "level": "sometimes"],
        ]
      ]
    ])
    #expect(reading.overview.accounts?.map(\.level) == [.lookOnly, .ask])
  }

  @Test("an older service's 404 for the questions reads as not offered")
  func anOlderServiceOffersNoQuestions() throws {
    let url = try #require(URL(string: "http://127.0.0.1:8765/abc/asks.json"))
    let missing = try #require(HTTPURLResponse(url: url, statusCode: 404, httpVersion: nil, headerFields: nil))
    #expect(throws: ServiceError.notOffered) { try ViewerClient.asks(Data("{}".utf8), response: missing) }
  }

  /// `asks.json`'s reply, from its fields.
  ///
  /// - Parameter reply: The reply's fields.
  /// - Returns: The list it reads as.
  /// - Throws: When it is not an object.
  private func decoded(_ reply: [String: Any]) throws -> AskList {
    let data = try JSONSerialization.data(withJSONObject: reply)
    return try JSONDecoder()
      .decode(AskList.self, from: data)
  }
}
