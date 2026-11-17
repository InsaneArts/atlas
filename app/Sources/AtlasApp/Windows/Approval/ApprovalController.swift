import AtlasCommands
import AtlasUI
import Foundation
import Observation

/// The approval window's one observed object: the request a command made of the person, and their answer. It reads
/// the request through the command line, shows it until its time runs out, and keeps the person's one answer through
/// the command line too, which alone judges it. Once kept, the answer holds for
/// ``AtlasUI/Motion/Duration/doneHold`` on the injected clock, or until Done, and the window closes. A way out while
/// the person may still answer refuses; nothing here allows without the person's press. ``ApprovalPresenter``
/// decides what the window says.
@Observable
final class ApprovalController {
  /// The request's ID, as the command line named it.
  @ObservationIgnored let id: String
  /// The request; nil until read, or when it could not be.
  private(set) var request: LifecycleRequest?
  /// Where the answer stands.
  private(set) var phase: ApprovalPhase = .loading
  /// The command line the window reads and answers through.
  @ObservationIgnored private let commands: any CommandRunning
  /// What the window asks of the rest of the app.
  @ObservationIgnored private let links: ApprovalLinks
  /// The wait for the request's time to run out, called off once it is answered.
  @ObservationIgnored private var expiry: Task<Void, Never>?
  /// The moment a kept answer holds before the window closes, which Done ends at once.
  @ObservationIgnored private let doneHold = DoneHold()
  /// Whether an answer was sent, so the person's answer is sent once however often a button is pressed.
  @ObservationIgnored private var answered = false

  /// Creates the approval window's controller.
  ///
  /// - Parameters:
  ///   - id: The request's ID.
  ///   - commands: The command line the window reads and answers through.
  ///   - links: What the window asks of the rest of the app.
  init(id: String, commands: any CommandRunning, links: ApprovalLinks) {
    self.id = id
    self.commands = commands
    self.links = links
  }

  /// The window as it should read now.
  var state: ApprovalState {
    ApprovalPresenter.state(ApprovalSnapshot(request: request, phase: phase))
  }

  /// Reads the request and shows it, then waits for its time to run out. A request the command line refuses, or
  /// one already past its time, shows why with Close alone.
  func start() async {
    switch LifecycleReading.of(await commands.run(.lifecycleRequest(id: id))) {
    case .failed(let reason):
      guard phase == .loading else { return }
      phase = .failed(reason)
    case .request(let request):
      guard phase == .loading else { return }
      self.request = request
      // An answered request says what was answered, before its time is judged: nothing is left to answer.
      if let answered = request.answered {
        phase = .answeredEarlier(ApprovalPresenter.earlier(answered))
        return
      }
      phase = .asking
      let left = request.until.timeIntervalSince(links.now())
      guard left > 0 else {
        phase = .expired
        return
      }
      let clock = links.clock
      // Unstructured, so the window answers while it waits; an answer calls it off.
      expiry = Task { [weak self] in
        do {
          try await clock.sleep(for: .milliseconds(Int64((left * 1000).rounded(.up))))
        } catch {
          return
        }
        self?.expire()
      }
    }
  }

  /// Applies what the person pressed: an answer, sent once, or Close.
  ///
  /// - Parameter choice: What they pressed.
  func send(_ choice: ApprovalChoice) {
    switch choice {
    case .allow, .refuse:
      // Unstructured, as `send` is the view's synchronous callback.
      Task { await answer(allow: choice == .allow) }
    case .close: close()
    }
  }

  /// Sends the person's answer, once: allowed or refused as they pressed, while the request still asks or is being
  /// read. What the command line kept shows, then the window closes after its hold; an answer it did not take says
  /// why, with Close alone.
  ///
  /// - Parameter allow: Whether the person allowed it.
  func answer(allow: Bool) async {
    guard !answered, phase == .asking || phase == .loading else { return }
    answered = true
    expiry?.cancel()
    phase = .answering(allow ? .allow : .refuse)
    switch LifecycleAnswer.of(await commands.run(.lifecycleAnswer(id: id, allow: allow))) {
    case .allowed: await hold(allowed: true)
    case .refused: await hold(allowed: false)
    case .failed(let reason): phase = .failed(reason)
    }
  }

  /// Whether the app may end now, as Quit asks: once nothing is left to answer or being kept. Until then it starts
  /// closing as Close does, refusing first while the person may still answer, and the window ends the app once it
  /// has.
  ///
  /// - Returns: Whether the app may end now.
  func mayQuit() -> Bool {
    switch phase {
    case .loading, .asking, .answering:
      close()
      return false
    case .answered, .answeredEarlier, .expired, .failed:
      return true
    }
  }

  /// The way out: once an answer holds, Done, which closes the window now; else the window closes, refusing first
  /// while the person may still answer, so closing never leaves the request asking.
  func close() {
    if doneHold.isHolding {
      doneHold.end()
      return
    }
    guard phase == .asking || phase == .loading else {
      if case .answering = phase { return }
      links.close()
      return
    }
    Task { await answer(allow: false) }
  }

  /// Shows the kept answer for its hold on the injected clock, or until Done, then closes the window.
  ///
  /// - Parameter allowed: Whether the person allowed it.
  private func hold(allowed: Bool) async {
    phase = .answered(allowed: allowed)
    await doneHold.hold(on: links.clock)
    links.close()
  }

  /// The request's time ran out before an answer: nothing changes, and the window offers Close alone.
  private func expire() {
    guard phase == .asking, !answered else { return }
    phase = .expired
  }
}
