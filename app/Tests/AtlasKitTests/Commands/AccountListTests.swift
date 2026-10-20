import Foundation
import Testing

@testable import AtlasKit

@Suite("AccountList")
struct AccountListTests {
  func list(_ reply: String) throws -> AccountList {
    let decoder = JSONDecoder()
    return try decoder.decode(AccountList.self, from: Data(reply.utf8))
  }

  @Test("the helper is the account atlas marks as the helper")
  func helper() throws {
    let marked = try list(
      #"{"ok": true, "accounts": [{"name": "me"}, {"name": "qa", "helper": false}, {"name": "atlas", "helper": true}]}"#
    )
    #expect(marked.helper == "atlas")
    #expect(try list(#"{"ok": true, "accounts": [{"name": "me"}, {"name": "qa", "helper": false}]}"#).helper == nil)
  }
}
