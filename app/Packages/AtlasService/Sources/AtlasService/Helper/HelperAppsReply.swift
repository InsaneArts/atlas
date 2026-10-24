/// `apps.json`'s body: the apps open in the helper, front to back, none when the list is missing or of the wrong type.
struct HelperAppsReply: Decodable {
  /// The apps, front to back.
  let apps: [HelperApp]

  /// The body's fields this app reads.
  enum CodingKeys: String, CodingKey { case apps }

  /// Decodes the body, keeping no apps when the list is missing or of the wrong type.
  ///
  /// - Parameter decoder: The decoder reading `apps.json`.
  /// - Throws: `DecodingError` only when the body is not an object.
  init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    apps = (try? container.decodeIfPresent([HelperApp].self, forKey: .apps)) ?? []
  }
}
