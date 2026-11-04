import Foundation

@testable import AtlasService

/// A frame service for tests: it answers each step of pairing, subscribing and settling as the real one does, and
/// answers credits as its script says.
actor ScriptedFramePipeBroker {
  /// How the broker answers a credit.
  enum Answer: Sendable {
    /// A full 4 × 2 picture whose bytes are its sequence.
    case picture
    /// The credit expires unserved.
    case expire
    /// The profile changes first: a state, then the credit is retired.
    case changeProfile
    /// A picture for another credit.
    case wrongCredit
    /// A fatal error.
    case refuse
    /// The picture first, its acceptance only after it.
    case lateAcceptance
    /// A state change between the picture's pieces.
    case interleaved
    /// The source stops working.
    case unavailable
    /// A picture whose commit comes too late: the credit is retired instead.
    case staleCommit
  }

  static let service = String(repeating: "a", count: 32)
  static let bootstrap = String(repeating: "b", count: 64)
  private static let observer = String(repeating: "c", count: 32)

  var control: ScriptedFramePipeLane?
  var frame: ScriptedFramePipeLane?
  /// The answers to credits still to come.
  var script: [Answer]
  /// Every packet received, by kind.
  private(set) var kinds: [FramePacketKind] = []
  private var stream: UInt64 = 1
  /// Whether the next commit is answered by retiring its credit.
  private var retireCommit = false
  private var sequence: UInt64 = 0

  init(script: [Answer]) {
    self.script = script
  }

  /// Both lanes, connected to this broker.
  func lanes() -> (ScriptedFramePipeLane, ScriptedFramePipeLane) {
    let control = ScriptedFramePipeLane(.control, broker: self)
    let frame = ScriptedFramePipeLane(.frame, broker: self)
    self.control = control
    self.frame = frame
    return (control, frame)
  }

  static var descriptor: FramePipeDescriptor {
    FramePipeDescriptor(
      controlPath: "/c",
      framePath: "/f",
      serviceInstance: service,
      observerBootstrap: bootstrap,
      brokerPID: 1,
      controllerUID: 501
    )
  }

  func received(_ data: Data, on lane: FramePipeLane) async {
    var codec = FramePacketCodec(lane: lane)
    var input = data
    guard case .packet(let header, let metadata)? = try? codec.next(&input),
      let asked = try? JSONSerialization.jsonObject(with: metadata) as? [String: Any]
    else { return }
    kinds.append(header.kind)
    let request = header.request
    switch header.kind {
    case .hello:
      reply(
        .welcome,
        request,
        [
          "serviceInstance": Self.service, "observerID": Self.observer, "observerGeneration": 1,
          "controlConnectionID": Self.service, "clientChallenge": asked["clientChallenge"] ?? "",
          "grantedRights": ["observe"], "acceptedUpdates": ["full"], "pairToken": Self.bootstrap,
          "pairExpiresInMs": 2_000, "cleanupToken": Self.bootstrap,
        ]
      )
    case .attach:
      var attached = asked
      attached["pairToken"] = nil
      attached["frameConnectionID"] = Self.service
      reply(.attached, request, attached, on: frame)
    case .pairConfirm: reply(.pairReady, request, asked)
    case .subscribe:
      reply(.subscribed, request, ["profile": profile, "barrierGeneration": 1])
    case .credit: await credit(asked, request)
    case .decoded: reply(.decodedAck, request, asked)
    case .commit where retireCommit:
      retireCommit = false
      reply(
        .error,
        request,
        ["failure": failure("source-stale"), "disposition": "credit-retired", "creditID": asked["creditID"] ?? 0]
      )
    case .commit: reply(.commitAck, request, asked)
    default: break
    }
  }

  private var profile: [String: Any] {
    [
      "streamGeneration": stream, "geometryGeneration": stream, "width": 4, "height": 2, "rowStride": 12,
      "displayWidth": 4.0, "displayHeight": 2.0, "displayScale": 1.0, "backend": "cg",
    ]
  }

  private func credit(_ asked: [String: Any], _ request: UInt64) async {
    let credit = asked["creditID"] as? UInt64 ?? 0
    let answer = script.isEmpty ? .picture : script.removeFirst()
    switch answer {
    case .lateAcceptance:
      sequence += 1
      picture(credit: credit, request: request)
      await frame?.drained()
      reply(.creditAccepted, request, ["creditID": credit])
      return
    case .interleaved:
      reply(.creditAccepted, request, ["creditID": credit])
      sequence += 1
      picture(credit: credit, request: request, split: true)
      return
    default: reply(.creditAccepted, request, ["creditID": credit])
    }
    switch answer {
    case .lateAcceptance, .interleaved: return
    case .unavailable:
      reply(
        .state,
        0,
        [
          "phase": "unavailable", "streamGeneration": stream, "geometryGeneration": stream, "inputReady": false,
          "controlOwner": "none", "failure": failure("source-stale"),
        ]
      )
    case .staleCommit:
      retireCommit = true
      sequence += 1
      picture(credit: credit, request: request)
    case .picture, .wrongCredit:
      sequence += 1
      picture(credit: answer == .picture ? credit : credit + 7, request: request)
    case .expire: reply(.creditExpired, request, ["creditID": credit])
    case .changeProfile:
      stream += 1
      reply(
        .state,
        0,
        [
          "phase": "profile-changed", "streamGeneration": stream, "geometryGeneration": stream,
          "inputReady": false, "controlOwner": "none", "profile": profile,
        ]
      )
      reply(
        .error,
        request,
        ["failure": failure("wrong-geometry"), "disposition": "credit-retired", "creditID": credit]
      )
    case .refuse:
      reply(.error, request, ["failure": failure("budget-exhausted"), "disposition": "pair-fatal"])
    }
  }

  private func failure(_ reason: String) -> [String: Any] {
    ["code": "protocol", "stage": "protocol", "uncertain": false, "dispatched": false, "reason": reason]
  }

  private func picture(credit: UInt64, request: UInt64, split: Bool = false) {
    let metadata: [String: Any] = [
      "serviceInstance": Self.service, "observerID": Self.observer, "observerGeneration": 1,
      "streamGeneration": stream, "geometryGeneration": stream, "creditID": credit, "sourceSequence": sequence,
      "imageSequence": sequence, "baseImageSequence": 0, "frameKey": String(repeating: "d", count: 64),
      "viewGeneration": 1, "inputRevision": 0, "width": 4, "height": 2, "rowStride": 12, "displayWidth": 4.0,
      "displayHeight": 2.0, "displayScale": 1.0, "ageAtHeaderNs": 1_000, "rectangleCount": 0,
    ]
    let pixels = Data(repeating: UInt8(sequence), count: 24)
    guard let body = try? JSONSerialization.data(withJSONObject: metadata),
      let head = try? FramePacketCodec.encode(kind: .fullFrame, request: request, metadata: body, payloadLength: 24)
    else { return }
    guard split else {
      frame?.send(head + pixels)
      return
    }
    frame?.send(head + pixels.prefix(10))
    reply(
      .state,
      0,
      [
        "phase": "live", "streamGeneration": stream, "geometryGeneration": stream, "inputReady": true,
        "controlOwner": "none",
      ]
    )
    Task { [frame] in
      await frame?.drained()
      frame?.send(pixels.dropFirst(10))
    }
  }

  private func reply(
    _ kind: FramePacketKind,
    _ request: UInt64,
    _ metadata: [String: Any],
    on lane: ScriptedFramePipeLane? = nil
  ) {
    guard let body = try? JSONSerialization.data(withJSONObject: metadata),
      let bytes = try? FramePacketCodec.encode(kind: kind, request: request, metadata: body, payloadLength: 0)
    else { return }
    let answering = lane ?? control
    answering?.send(bytes)
  }
}
