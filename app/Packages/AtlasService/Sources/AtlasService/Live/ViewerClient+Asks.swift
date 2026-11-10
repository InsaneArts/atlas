import Foundation

/// The person's questions and their answers: reading `asks.json`, answering one, ending what they allowed, and the
/// level of an account.
extension ViewerClient {
  /// Reads `asks.json`.
  ///
  /// - Returns: The questions, oldest first, and the grants, every missing field at its safe default.
  /// - Throws: ``ServiceError/notOffered`` when an older service answers 404; else ``ServiceError`` with the service's
  ///   error, or when it cannot be read.
  public func asks() async throws(ServiceError) -> AskList {
    let route = base.appendingPathComponent("asks.json")
    let (data, response) = try await fetched(request(to: route), limit: Self.listLimit)
    do {
      return try await Self.decodedReply { () throws(ServiceError) in try Self.asks(data, response: response) }
    } catch {
      throw unsecured(error)
    }
  }

  /// An `asks.json` reply read: only a 200 carries questions, so a refusal never reads as none.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The questions and the grants.
  /// - Throws: ``ServiceError/notOffered`` for a 404; ``ServiceError`` with the service's error for any other reply
  ///   that is not a 200, or when its body is not an object.
  static func asks(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> AskList {
    guard response.statusCode != 404 else { throw .notOffered }
    return try listed(AskList.self, data, response: response, fallback: "Atlas couldn't read its questions.")
  }

  /// Posts to `asks/answer`.
  ///
  /// - Parameters:
  ///   - ask: The question's id.
  ///   - answer: The answer.
  /// - Throws: ``ServiceError`` when the service refuses.
  public func answer(_ ask: String, with answer: AskAnswer) async throws {
    /// The request: the question and the answer.
    struct Body: Encodable {
      /// The question's id.
      let id: String
      /// `task`, `once` or `deny`.
      let choice: AskAnswer
    }
    _ = try await post("asks/answer", Body(id: ask, choice: answer))
  }

  /// Posts to `allowed/end`.
  ///
  /// - Parameter account: The account whose grants end.
  /// - Throws: ``ServiceError`` when the service refuses.
  public func endAllowed(in account: AccountKey) async throws {
    /// The request: the account, as the service named it.
    struct Body: Encodable {
      /// The account.
      let account: AccountKey
    }
    _ = try await post("allowed/end", Body(account: account))
  }

  /// Posts to `accounts/level`.
  ///
  /// - Parameters:
  ///   - level: The level.
  ///   - account: The account.
  /// - Throws: ``ServiceError`` when the service refuses.
  public func setLevel(_ level: AccountLevel, of account: ViewerStatus.Account) async throws {
    /// The request: the account by its kind, key and browser, and the level.
    struct Body: Encodable {
      /// `site` or `app`.
      let kind: String
      /// The site's domain, or the app's bundle identifier.
      let id: String
      /// For a site, the browser it was seen in.
      let browser: String?
      /// `lookOnly`, `ask` or `trusted`.
      let level: AccountLevel
    }
    _ = try await post(
      "accounts/level",
      Body(kind: account.kind.rawValue, id: account.key, browser: account.browser, level: level)
    )
  }
}
