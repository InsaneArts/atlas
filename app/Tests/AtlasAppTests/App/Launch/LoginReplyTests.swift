import Foundation
import Testing

@testable import AtlasApp

@Suite("LoginReply")
struct LoginReplyTests {
  /// A reply as `atlas menubar --login` reads it: a JSON object, or nil when it is not one.
  ///
  /// - Parameter reply: The reply.
  /// - Returns: Its fields.
  private static func read(_ reply: LoginReply) -> [String: Any]? {
    (try? JSONSerialization.jsonObject(with: reply.json)) as? [String: Any]
  }

  @Test("where Open at Login stands is reported as ok, enabled and available, and nothing more")
  func theStateIsReported() throws {
    let fields = try #require(Self.read(.state(OpenAtLogin.State(isAvailable: true, isEnabled: false))))
    #expect(Set(fields.keys) == ["ok", "enabled", "available"])
    #expect(fields["ok"] as? Bool == true)
    #expect(fields["enabled"] as? Bool == false)
    #expect(fields["available"] as? Bool == true)
  }

  @Test("a failure's reason stays valid JSON, word for word, whatever it holds")
  func aFailureStaysValid() throws {
    let reason = #"The "login item" at C:\path failed"# + "\nTry again."
    let fields = try #require(Self.read(.failure(reason)))
    #expect(Set(fields.keys) == ["ok", "error"])
    #expect(fields["ok"] as? Bool == false)
    #expect(fields["error"] as? String == reason)
  }
}
