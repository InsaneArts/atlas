import AtlasService

/// Input owned by the lease that made it, even across slow replies: sent in
/// order, one batch at a time; a newer move replaces a queued one; too much
/// waiting at once ends control instead of falling behind.
@MainActor
public final class InputQueue {
  /// The most batches that may wait; one more ends control.
  static let limit = 32

  /// One batch waiting, with the lease and generation it was queued under.
  struct Item {
    let actions: [InputAction]
    let frame: String
    let lease: String
    let generation: Int
  }

  /// Where the input goes, and who hears how it went.
  public weak var sink: (any InputSink)?
  private(set) var owner: String?
  private var generation = 0
  private(set) var pending: [Item] = []
  private var sending = false

  /// Whether a batch that scrolls still waits to be sent.
  public var holdsScroll: Bool {
    pending.contains { item in item.actions.contains { $0.kind == "scroll" } }
  }

  /// Creates a queue that owns no lease yet.
  public init() {}

  /// Input from now on belongs to `owner`; anything queued is dropped.
  ///
  /// - Parameter owner: The lease that owns new input, or `nil` to queue nothing.
  public func activate(_ owner: String?) {
    generation += 1
    self.owner = owner
    pending = []
  }

  /// Queues a batch for the current lease and starts sending.
  ///
  /// - Parameters:
  ///   - actions: The batch, in order.
  ///   - frame: The id of the picture its coordinates refer to.
  public func enqueue(_ actions: [InputAction], frame: String) {
    guard let owner else { return }
    if actions.count == 1, let last = pending.last, last.actions.count == 1, actions[0].isMove,
      last.actions[0].isMove, actions[0].kind == last.actions[0].kind
    {
      pending.removeLast()
    }
    guard pending.count < Self.limit else {
      sink?.inputFailed(
        ServiceError(
          "Input is arriving faster than the helper can handle it. Take control again when ready."
        )
      )
      return
    }
    pending.append(Item(actions: actions, frame: frame, lease: owner, generation: generation))
    // Unstructured, as input arrives through synchronous event handlers; `sending` keeps one drain at a time.
    Task { await drain() }
  }

  /// Sends what waits, one batch at a time, while a lease owns the queue; a batch of an older lease is dropped.
  func drain() async {
    guard !sending else { return }
    sending = true
    while owner != nil, !pending.isEmpty, let sink {
      let item = pending.removeFirst()
      guard item.generation == generation else { continue }
      do {
        try await sink.send(lease: item.lease, frame: item.frame, actions: item.actions)
        if item.generation == generation { sink.inputSent() }
      } catch {
        if item.generation == generation { sink.inputFailed(error) }
      }
    }
    sending = false
  }
}
