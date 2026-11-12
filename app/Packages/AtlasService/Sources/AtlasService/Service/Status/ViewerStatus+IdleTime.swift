extension ViewerStatus {
  /// The idle time the person's apps read, as the service found it in the person's login. Password managers lock on it
  /// and chat apps mark the person away. A switch to or from the helper can stop it while an app keeps secure keyboard
  /// entry on; the service restarts it when it can, and names the app keeping it stopped when one does.
  public struct IdleTime: Decodable, Equatable, Sendable {
    /// How it stands.
    public enum State: String, Decodable, Equatable, Sendable {
      /// It follows the person's input.
      case live
      /// It stopped while the person was at the Mac, and the service could not restart it.
      case stuck
      /// It stopped while the person was at the Mac because an app keeps secure keyboard entry on, named in
      /// ``IdleTime/app`` when it has a name: quitting that app restarts it.
      case held
      /// The service could not tell.
      case unknown
    }

    /// How it stands.
    public var state = State.unknown
    /// The app keeping it stopped, by the name it shows, while it is held by one with a name.
    public var app: String?

    /// Creates an idle time.
    ///
    /// - Parameters:
    ///   - state: How it stands; unknown by default.
    ///   - app: The app keeping it stopped; none by default.
    public init(state: State = .unknown, app: String? = nil) {
      self.state = state
      self.app = app
    }

    /// The fields the service writes.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case state, app
    }

    /// Decodes an idle time, keeping the default of each field that is missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading it.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      state = (try? container.decodeIfPresent(State.self, forKey: .state)) ?? .unknown
      app = try? container.decodeIfPresent(String.self, forKey: .app)
    }
  }
}
