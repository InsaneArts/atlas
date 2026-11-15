/// What a station asks a VM's runner on its control socket: a read, or a
/// change with its ID.
public enum ControlOperation: Equatable, Sendable {
  /// The runner's version and what it can do.
  case hello
  /// The VM's power, as the runner sees it.
  case state
  /// What the change with this ID came to.
  case outcome(ChangeID)
  /// Make the VM.
  case create(ChangeID)
  /// Start the VM.
  case start(ChangeID)
  /// Stop the VM.
  case stop(ChangeID)
  /// Open the VM's window with input, or close it.
  case window(ChangeID, input: Bool)

  /// The change it is; none for a read.
  public var change: ChangeID? {
    switch self {
    case .hello, .state, .outcome:
      nil
    case .create(let change), .start(let change), .stop(let change), .window(let change, _):
      change
    }
  }

  /// Its name on the socket.
  public var name: String {
    switch self {
    case .hello: "hello"
    case .state: "state"
    case .outcome: "outcome"
    case .create: "create"
    case .start: "start"
    case .stop: "stop"
    case .window: "window"
    }
  }
}

extension ControlOperation: Codable {
  private enum Key: String, CodingKey {
    case op
    case change
    case input
  }

  /// Read from one request: its `op`, and the `change` and `input` it needs.
  public init(from decoder: any Decoder) throws {
    let fields = try decoder.container(keyedBy: Key.self)
    let name = try fields.decode(String.self, forKey: .op)
    switch name {
    case "hello":
      self = .hello
    case "state":
      self = .state
    case "outcome":
      self = .outcome(try fields.decode(ChangeID.self, forKey: .change))
    case "create":
      self = .create(try fields.decode(ChangeID.self, forKey: .change))
    case "start":
      self = .start(try fields.decode(ChangeID.self, forKey: .change))
    case "stop":
      self = .stop(try fields.decode(ChangeID.self, forKey: .change))
    case "window":
      self = .window(
        try fields.decode(ChangeID.self, forKey: .change),
        input: try fields.decode(Bool.self, forKey: .input)
      )
    default:
      throw DecodingError.dataCorruptedError(
        forKey: .op,
        in: fields,
        debugDescription: "not an operation of a VM's runner"
      )
    }
  }

  /// Written as one request reads.
  public func encode(to encoder: any Encoder) throws {
    var fields = encoder.container(keyedBy: Key.self)
    try fields.encode(name, forKey: .op)
    switch self {
    case .hello, .state:
      break
    case .outcome(let change), .create(let change), .start(let change), .stop(let change):
      try fields.encode(change, forKey: .change)
    case .window(let change, let input):
      try fields.encode(change, forKey: .change)
      try fields.encode(input, forKey: .input)
    }
  }
}
