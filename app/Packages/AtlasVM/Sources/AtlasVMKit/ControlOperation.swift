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
  /// Put Atlas inside the VM's guest.
  case install(ChangeID, InstallRequest)
  /// Go on with the install of this change, already handed its secret,
  /// asked again without it: as far as its close, the guest account taken
  /// out of the administrators and Remote Login turned off again.
  case installResume(ChangeID)
  /// The station's word that the guest of the install change `install`
  /// paired for `enrolment`.
  case paired(ChangeID, install: ChangeID, enrolment: String)
  /// The station's word that the guest of the install change `install`
  /// says, over its authenticated session, that Remote Login is off and its
  /// account no administrator.
  case closed(ChangeID, install: ChangeID)
  /// The VM removed: from its asking no other change is taken, the VM is
  /// stopped and its passwords deleted, and then its runner exits.
  case forget(ChangeID)
  /// Make sure the change with this ID is never done from now: one not yet
  /// asked is kept as refused, so a request of it that comes later is never
  /// done, and one asked is answered as what it came to.
  case void(ChangeID)

  /// The change it is; none for a read.
  public var change: ChangeID? {
    switch self {
    case .hello, .state, .outcome:
      nil
    case .create(let change), .start(let change), .stop(let change), .window(let change, _),
      .install(let change, _), .installResume(let change), .paired(let change, _, _), .closed(let change, _),
      .forget(let change), .void(let change):
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
    case .install: "install"
    case .installResume: "install-resume"
    case .paired: "paired"
    case .closed: "closed"
    case .forget: "forget"
    case .void: "void"
    }
  }

  /// It as a record keeps it: an install's secret as its digest alone.
  public var recorded: ControlOperation {
    guard case .install(let change, let request) = self else { return self }
    return .install(change, request.recorded)
  }

  /// Whether, left pending, it goes on when asked again: an install, whose
  /// phases are each safe to repeat, and the station's reports on one.
  var resumes: Bool {
    switch self {
    case .install, .paired, .closed, .forget: true
    default: false
    }
  }

  /// Whether it is the removal's `forget`.
  var isForget: Bool {
    if case .forget = self { return true }
    return false
  }

  /// Whether it is the station's report on an install, which touches only
  /// that install's record and never the VM.
  public var isReport: Bool {
    switch self {
    case .paired, .closed: true
    default: false
    }
  }
}

extension ControlOperation: Codable {
  private enum Key: String, CodingKey {
    case op
    case change
    case input
    case bundle
    case enrolment
    case secret
    case install
  }

  /// Read from one request: its `op`, and the fields it needs.
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
    case "forget":
      self = .forget(try fields.decode(ChangeID.self, forKey: .change))
    case "void":
      self = .void(try fields.decode(ChangeID.self, forKey: .change))
    case "install-resume":
      self = .installResume(try fields.decode(ChangeID.self, forKey: .change))
    case "window":
      self = .window(
        try fields.decode(ChangeID.self, forKey: .change),
        input: try fields.decode(Bool.self, forKey: .input)
      )
    case "install":
      self = .install(
        try fields.decode(ChangeID.self, forKey: .change),
        InstallRequest(
          bundle: try fields.decode(String.self, forKey: .bundle),
          enrolment: try fields.decode(String.self, forKey: .enrolment),
          secret: try fields.decode(String.self, forKey: .secret)
        )
      )
    case "paired":
      self = .paired(
        try fields.decode(ChangeID.self, forKey: .change),
        install: try fields.decode(ChangeID.self, forKey: .install),
        enrolment: try fields.decode(String.self, forKey: .enrolment)
      )
    case "closed":
      self = .closed(
        try fields.decode(ChangeID.self, forKey: .change),
        install: try fields.decode(ChangeID.self, forKey: .install)
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
    case .outcome(let change), .create(let change), .start(let change), .stop(let change), .forget(let change),
      .installResume(let change), .void(let change):
      try fields.encode(change, forKey: .change)
    case .window(let change, let input):
      try fields.encode(change, forKey: .change)
      try fields.encode(input, forKey: .input)
    case .install(let change, let request):
      try fields.encode(change, forKey: .change)
      try fields.encode(request.bundle, forKey: .bundle)
      try fields.encode(request.enrolment, forKey: .enrolment)
      try fields.encode(request.secret, forKey: .secret)
    case .paired(let change, let install, let enrolment):
      try fields.encode(change, forKey: .change)
      try fields.encode(install, forKey: .install)
      try fields.encode(enrolment, forKey: .enrolment)
    case .closed(let change, let install):
      try fields.encode(change, forKey: .change)
      try fields.encode(install, forKey: .install)
    }
  }
}
