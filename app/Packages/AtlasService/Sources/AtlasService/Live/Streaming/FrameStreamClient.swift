import Foundation

/// One observer of the frame service: it pairs both lanes, subscribes, and reads one verified picture per credit,
/// which only its commit makes the picture input may name. Every reply is checked against exactly what was asked;
/// anything else ends the observer.
public actor FrameStreamClient {
  private let descriptor: FramePipeDescriptor
  private let control: any FramePipeTransport
  private let frame: any FramePipeTransport
  /// What both lanes' readers decoded, in arrival order per lane.
  private let incoming: AsyncThrowingStream<FramePipeIncoming, any Error>
  private let readers: [Task<Void, Never>]
  /// The last request ID sent on the control lane.
  private var request: UInt64 = 0
  /// The last credit given.
  private var credit: UInt64 = 0
  /// The observer this client was welcomed as.
  private var observerID = ""
  /// The accepted profile's generations, and the barrier its subscription installed.
  private var generations: (stream: UInt64, geometry: UInt64, barrier: UInt64)?
  /// Whether the next credit must wait for a picture taken after the last subscription.
  private var fresh = true

  /// Creates an observer over two connected lanes; it reads them from now on.
  ///
  /// - Parameters:
  ///   - descriptor: The service the lanes reach.
  ///   - control: The control lane.
  ///   - frame: The frame lane.
  public init(descriptor: FramePipeDescriptor, control: any FramePipeTransport, frame: any FramePipeTransport) {
    self.descriptor = descriptor
    self.control = control
    self.frame = frame
    let (stream, sink) = AsyncThrowingStream<FramePipeIncoming, any Error>.makeStream()
    incoming = stream
    readers = [
      Self.read(control, lane: .control, into: sink),
      Self.read(frame, lane: .frame, into: sink),
    ]
  }

  /// Decodes `lane` into `sink` until it ends or fails, which ends the sink.
  private static func read(
    _ lane: any FramePipeTransport,
    lane kind: FramePipeLane,
    into sink: AsyncThrowingStream<FramePipeIncoming, any Error>.Continuation
  ) -> Task<Void, Never> {
    Task {
      var codec = FramePacketCodec(lane: kind)
      do {
        while true {
          var input = try await lane.read(upTo: FramePipeLimits.chunk)
          guard !input.isEmpty else { break }
          while let item = try codec.next(&input) {
            switch item {
            case .packet(let header, let metadata): sink.yield(.packet(kind, header, metadata))
            case .payload(let piece): sink.yield(.payload(piece))
            }
          }
        }
        try codec.end()
        sink.finish(throwing: FramePipeError.truncated)
      } catch {
        sink.finish(throwing: error)
      }
    }
  }

  /// Pairs both lanes and subscribes for pictures at most `longest` pixels on their longest side.
  ///
  /// - Parameter longest: The detail asked for, 320 to 4,096 pixels.
  /// - Throws: ``FramePipeError`` or a lane failure when the service refuses or answers anything but the next step.
  public func open(longest: Int) async throws {
    let challenge = Self.randomHex(32)
    try await send(
      .hello,
      on: control,
      [
        "protocolMajor": Int(FramePipeLimits.major), "serviceInstance": descriptor.serviceInstance,
        "bootstrapCapability": descriptor.observerBootstrap, "clientChallenge": challenge, "role": "observer",
        "clientKind": "swift", "requestedRights": ["observe"], "offeredUpdates": ["full"],
      ]
    )
    let welcome = try await expect(
      .welcome,
      keys: [
        "serviceInstance", "observerID", "observerGeneration", "controlConnectionID", "clientChallenge",
        "grantedRights", "acceptedUpdates", "pairToken", "pairExpiresInMs", "cleanupToken",
      ]
    )
    guard welcome["clientChallenge"] as? String == challenge,
      welcome["serviceInstance"] as? String == descriptor.serviceInstance,
      let observer = welcome["observerID"] as? String,
      let generation = welcome["observerGeneration"] as? UInt64,
      let connection = welcome["controlConnectionID"] as? String,
      let token = welcome["pairToken"] as? String
    else { throw FramePipeError.invalid }
    observerID = observer
    let frameChallenge = Self.randomHex(32)
    let ids: [String: Any] = [
      "serviceInstance": descriptor.serviceInstance, "observerID": observer, "observerGeneration": generation,
      "controlConnectionID": connection,
    ]
    try await send(
      .attach,
      on: frame,
      request: 1,
      ids.merging(["frameChallenge": frameChallenge, "pairToken": token]) { $1 }
    )
    let attached = try await expect(
      .attached,
      keys: Set(ids.keys).union(["frameChallenge", "frameConnectionID"]),
      lane: .frame
    )
    guard attached["frameChallenge"] as? String == frameChallenge,
      let frameConnection = attached["frameConnectionID"] as? String
    else { throw FramePipeError.invalid }
    let pair = ids.merging(["frameConnectionID": frameConnection]) { $1 }
    try await send(.pairConfirm, on: control, pair)
    let ready = try await expect(.pairReady, keys: Set(pair.keys))
    guard ready["frameConnectionID"] as? String == frameConnection, ready["observerID"] as? String == observer
    else { throw FramePipeError.invalid }
    try await subscribe(reason: "first", longest: longest)
  }

  /// Subscribes, or subscribes again under a new profile, and keeps the accepted profile.
  private func subscribe(reason: String, longest: Int) async throws {
    try await send(
      .subscribe,
      on: control,
      ["reason": reason, "demand": ["desiredLongest": longest, "backend": "cg"], "viewGeneration": 1]
    )
    let subscribed = try await expect(.subscribed, keys: ["profile", "barrierGeneration"])
    guard let profile = subscribed["profile"] as? [String: Any],
      let stream = profile["streamGeneration"] as? UInt64,
      let geometry = profile["geometryGeneration"] as? UInt64,
      let barrier = subscribed["barrierGeneration"] as? UInt64
    else { throw FramePipeError.invalid }
    generations = (stream, geometry, barrier)
    fresh = true
  }

  /// The next picture: one credit, then the picture it earns. A credit that expires unserved is given again; a
  /// changed profile is subscribed to again first.
  ///
  /// - Parameter longest: The detail asked for, should the profile need subscribing to again.
  /// - Returns: The picture, not yet committed.
  /// - Throws: ``FramePipeError`` or a lane failure, which end the observer.
  public func next(longest: Int) async throws -> FramePipePicture {
    while true {
      if generations == nil { try await subscribe(reason: "reconfigure", longest: longest) }
      guard let generations else { throw FramePipeError.invalid }
      credit += 1
      var asked: [String: Any] = [
        "creditID": credit, "viewGeneration": 1, "streamGeneration": generations.stream,
        "geometryGeneration": generations.geometry, "barrier": fresh ? "subscribe" : "ordinary",
      ]
      if fresh { asked["barrierGeneration"] = generations.barrier }
      try await send(.credit, on: control, asked)
      var iterator = incoming.makeAsyncIterator()
      reading: while true {
        // An ended stream, or a cancelled wait, ends the observer: never another credit.
        guard let item = try await iterator.next() else { throw FramePipeError.truncated }
        switch item {
        case .packet(.control, let header, let metadata):
          switch header.kind {
          case .creditAccepted: continue
          case .creditExpired: break reading
          case .state: try note(metadata)
          case .error:
            if self.generations == nil, Self.retired(metadata) {
              try await subscribe(reason: "reconfigure", longest: longest)
              break reading
            }
            throw Self.refusal(metadata)
          default: throw FramePipeError.invalid
          }
        case .packet(.frame, let header, let metadata) where header.kind == .fullFrame:
          let picture = try FramePictureMetadata(validating: metadata)
          try picture.check(kind: .fullFrame, payloadLength: header.payloadLength)
          guard picture.creditID == credit, picture.observerID == observerID else { throw FramePipeError.invalid }
          var rgb = Data(capacity: Int(header.payloadLength))
          // The lanes are independent: control packets may arrive between the picture's pieces.
          while rgb.count < Int(header.payloadLength) {
            switch try await iterator.next() {
            case .payload(let piece): rgb.append(piece)
            case .packet(.control, let header, let metadata): try crossing(header, metadata)
            default: throw FramePipeError.truncated
            }
          }
          fresh = false
          return try Self.picture(picture, rgb: rgb)
        default: throw FramePipeError.invalid
        }
      }
    }
  }

  /// Commits `picture`: decoded, then selected, so input may name it.
  ///
  /// - Parameter picture: The picture shown.
  /// - Throws: ``FramePipeError`` or a lane failure when the service does not acknowledge both exactly.
  public func commit(_ picture: FramePipePicture) async throws {
    let reference = try JSONSerialization.jsonObject(with: picture.reference)
    let settle: [String: Any] = ["creditID": picture.credit, "ref": reference]
    try await send(.decoded, on: control, settle)
    try await send(.commit, on: control, settle)
    for kind in [FramePacketKind.decodedAck, .commitAck] {
      let echo = try await expect(kind, keys: ["creditID", "ref"])
      guard echo["creditID"] as? UInt64 == picture.credit else { throw FramePipeError.invalid }
    }
  }

  /// Closes both lanes; the service ends the observer.
  public func close() {
    control.close()
    frame.close()
    for reader in readers { reader.cancel() }
  }

  /// The picture `metadata` and `rgb` describe.
  private static func picture(_ metadata: FramePictureMetadata, rgb: Data) throws -> FramePipePicture {
    let reference: [String: Any] = [
      "serviceInstance": metadata.serviceInstance, "observerID": metadata.observerID,
      "observerGeneration": metadata.observerGeneration, "streamGeneration": metadata.streamGeneration,
      "geometryGeneration": metadata.geometryGeneration, "sourceSequence": metadata.sourceSequence,
      "imageSequence": metadata.imageSequence, "frameKey": metadata.frameKey,
      "viewGeneration": metadata.viewGeneration,
    ]
    return FramePipePicture(
      rgb: rgb,
      width: Int(metadata.width),
      height: Int(metadata.height),
      display: ViewerStatus.Size(width: metadata.displayWidth, height: metadata.displayHeight),
      frameKey: metadata.frameKey,
      revision: "pipe:\(metadata.streamGeneration):\(metadata.geometryGeneration)",
      ageAtHeader: Double(metadata.ageAtHeaderNs) / 1e9,
      credit: metadata.creditID,
      reference: try JSONSerialization.data(withJSONObject: reference)
    )
  }

  /// Sends a `kind` packet with `metadata` on `lane`, as the next request unless `request` names one.
  private func send(
    _ kind: FramePacketKind,
    on lane: any FramePipeTransport,
    request fixed: UInt64? = nil,
    _ metadata: [String: Any]
  ) async throws {
    let id: UInt64
    if let fixed {
      id = fixed
    } else {
      request += 1
      id = request
    }
    let body = try JSONSerialization.data(withJSONObject: metadata)
    let head = try FramePacketCodec.encode(kind: kind, request: id, metadata: body, payloadLength: 0)
    try await lane.write(head)
  }

  /// The next packet, which must be a `kind` on `lane` with exactly `keys`.
  private func expect(
    _ kind: FramePacketKind,
    keys: Set<String>,
    lane: FramePipeLane = .control
  ) async throws -> [String: Any] {
    var iterator = incoming.makeAsyncIterator()
    while true {
      guard case .packet(let arrived, let header, let metadata) = try await iterator.next() else {
        throw FramePipeError.truncated
      }
      if arrived == lane, header.kind == kind { return try Self.object(metadata, keys: keys) }
      guard arrived == .control else { throw FramePipeError.invalid }
      try crossing(header, metadata)
    }
  }

  /// Handles a control packet that crossed a reply or a picture: an acceptance needs nothing, a state change is
  /// noted, and anything else ends the observer.
  private func crossing(_ header: FramePipeHeader, _ metadata: Data) throws {
    switch header.kind {
    case .creditAccepted: return
    case .state: try note(metadata)
    case .error: throw Self.refusal(metadata)
    default: throw FramePipeError.invalid
    }
  }

  /// Notes a state change: a changed profile is subscribed to again before the next credit, and a source that
  /// stopped working ends the observer, so the window reads pictures another way meanwhile.
  private func note(_ metadata: Data) throws {
    let state = try Self.object(metadata, keys: nil)
    switch state["phase"] as? String {
    case "profile-changed": generations = nil
    case "unavailable": throw ServiceError("The helper's pictures are unavailable")
    default: return
    }
  }

  /// `metadata` as an object, once it passes the lexical check and, when `keys` is given, has exactly those keys.
  private static func object(_ metadata: Data, keys: Set<String>?) throws -> [String: Any] {
    let found = try FrameJSONValidator.validate(metadata)
    if let keys, Set(found.keys) != Set(keys.map { Array($0.utf8) }) { throw FramePipeError.invalid }
    guard let object = try JSONSerialization.jsonObject(with: metadata) as? [String: Any] else {
      throw FramePipeError.invalid
    }
    return object
  }

  /// Whether an error only retired a credit.
  private static func retired(_ metadata: Data) -> Bool {
    (try? object(metadata, keys: nil))?["disposition"] as? String == "credit-retired"
  }

  /// The service's refusal, in its fixed words.
  private static func refusal(_ metadata: Data) -> ServiceError {
    let failure = (try? object(metadata, keys: nil))?["failure"] as? [String: Any]
    let reason = failure?["reason"] as? String ?? "other-known-error"
    return ServiceError("The frame service refused: \(reason)")
  }

  /// `digits` random lowercase hex digits.
  private static func randomHex(_ digits: Int) -> String {
    var generator = SystemRandomNumberGenerator()
    return (0..<digits).map { _ in String(Int.random(in: 0..<16, using: &generator), radix: 16) }.joined()
  }
}
