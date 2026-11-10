import AtlasService
import AtlasStatus
import AtlasUI
import Foundation
import Observation

/// The questions Atlas asks the person before an agent sends, and what they allowed, as every window shows them: read
/// from `asks.json` when the asks folder changes, when the status lists other questions than it holds, and when the
/// earliest question or grant ends, so one that ran out goes on time (reading it takes it down; the app never deletes
/// a question's file). An answer given in the panel, the viewer or a notification takes the question away here first,
/// so every surface clears in the same frame, then goes to the service. ``HelperActions`` holds the one every window
/// shares.
@Observable
final class AskStore {
  /// What `asks.json` last said, less the questions answered here since; nil before the first reading, while the
  /// windows show the status's own summary.
  private(set) var list: AskList?
  /// The pictures of the questions up, decoded, by their file name: a rerun that replaces a picture names it anew, so
  /// a question shows only the picture its own reading named, beside the frame that reading gave.
  private(set) var pictures: [String: DesktopPicture] = [:]
  /// The viewer service it reads and answers through.
  @ObservationIgnored private let service: any ViewerService
  /// The clock the wait for the earliest end runs on.
  @ObservationIgnored private let clock: any Clock<Duration>
  /// The time now, which ends are counted to.
  @ObservationIgnored private let wallClock: @MainActor () -> Date
  /// Where the questions and their pictures are kept.
  @ObservationIgnored private var folder = AskFolder.none
  /// The asks folders' watch, while it runs.
  @ObservationIgnored private var watching: AskFolderWatch?
  /// Told each time the questions change, as the notifications follow them.
  @ObservationIgnored var changed: @MainActor (AskList) -> Void = { _ in }
  /// The questions answered here whose answer a reading started before it may not know yet, kept hidden.
  @ObservationIgnored private var answered: Set<String> = []
  /// Numbers each reading, so only the newest one started is shown.
  @ObservationIgnored private var reading = 0
  /// Whether a reading is asked for and not started yet, so a burst of changes reads once.
  @ObservationIgnored private var readSoon = false
  /// The wait for the earliest end, while one runs.
  @ObservationIgnored private var wake: Task<Void, Never>?
  /// Whether the service offers `asks.json`; an older one never asks, so it is not read again.
  @ObservationIgnored private var offered = true

  /// A store that has read nothing yet.
  ///
  /// - Parameters:
  ///   - service: The viewer service.
  ///   - clock: The clock the wait for the earliest end runs on.
  ///   - wallClock: The time now; the system's by default.
  init(
    service: any ViewerService,
    clock: any Clock<Duration> = ContinuousClock(),
    wallClock: @escaping @MainActor () -> Date = { Date() }
  ) {
    self.service = service
    self.clock = clock
    self.wallClock = wallClock
  }

  /// Starts hearing the asks folders, reading the questions at each change and once now.
  ///
  /// - Parameter folder: Where the questions are kept.
  func watch(_ folder: AskFolder) {
    self.folder = folder
    watching = folder.watch { [weak self] in self?.askRead() }
    askRead()
  }

  /// A status was read: the questions are read again when it lists others than the store holds.
  ///
  /// - Parameter status: The service's answer.
  func statusRead(_ status: ViewerStatus) {
    if offered, StatusReading(status).asksChanged(since: list) { askRead() }
  }

  /// Reads the questions on the next turn, once however many changes come before it.
  func askRead() {
    guard !readSoon else { return }
    readSoon = true
    // Unstructured, as a folder's change and a status's reading are synchronous callbacks.
    Task { [weak self] in
      self?.readSoon = false
      await self?.read()
    }
  }

  /// Reads the questions now. A reading overtaken by a newer one shows nothing; one that fails keeps what was there,
  /// and an older service that offers none reads as none.
  func read() async {
    reading += 1
    let number = reading
    let read: AskList
    do {
      read = try await service.asks()
    } catch  where error.isNotOffered {
      offered = false
      read = AskList()
    } catch {
      return
    }
    guard number == reading else { return }
    adopt(read)
    watching?.rescan()
  }

  /// Answers a question as the person: it goes from every window at once, then the answer goes to the service, and
  /// the questions are read again, so one the service refused, as one that ended, shows as the service has it.
  ///
  /// - Parameters:
  ///   - id: The question's id.
  ///   - answer: The answer.
  func answer(_ id: String, with answer: AskAnswer) async {
    hide(id)
    do {
      try await service.answer(id, with: answer)
    } catch {
      answered.remove(id)
    }
    await read()
  }

  /// Ends what the person allowed for a task: it goes at once, then the service ends it.
  ///
  /// - Parameter id: The grant's id.
  func end(_ id: String) async {
    guard let grant = list?.allowed.first(where: { $0.id == id }) else { return }
    list?.allowed.removeAll { $0.account == grant.account }
    try? await service.endAllowed(in: grant.account)
    await read()
  }

  /// Takes a question away from every window.
  ///
  /// - Parameter id: The question's id.
  private func hide(_ id: String) {
    answered.insert(id)
    list?.asks.removeAll { $0.id == id }
    keepPictures()
    if let list { changed(list) }
  }

  /// Shows a reading, less what was answered here and what has ended, reads the pictures of new questions and waits for the earliest
  /// end.
  ///
  /// - Parameter read: The reading.
  private func adopt(_ read: AskList) {
    let listed = Set(read.asks.map(\.id))
    answered.formIntersection(listed)
    var shown = read
    // A reply that arrives late may still hold what ended meanwhile: it shows nowhere, and no notification posts for it.
    let now = wallClock()
    shown.asks.removeAll { answered.contains($0.id) || $0.ends <= now }
    shown.allowed.removeAll { $0.ends <= now }
    list = shown
    keepPictures()
    for name in shown.asks.compactMap(\.picture) where pictures[name] == nil {
      load(name)
    }
    waitForEnd(of: shown)
    changed(shown)
  }

  /// Keeps only the pictures a question up now names.
  private func keepPictures() {
    let named = Set(list?.asks.compactMap(\.picture) ?? [])
    pictures = pictures.filter { named.contains($0.key) }
  }

  /// Reads and decodes a question's picture off the main actor, keeping it only while a question still names it.
  ///
  /// - Parameter name: The picture's file name.
  private func load(_ name: String) {
    let folder = folder
    // Unstructured, as a reading shows its questions at once and their pictures as they decode.
    Task { [weak self] in
      guard let data = await folder.picture(name), let image = await ImageDecoding.image(data) else { return }
      guard let self, self.list?.asks.contains(where: { $0.picture == name }) == true else { return }
      self.pictures[name] = DesktopPicture(id: "ask-\(name)", image: image)
    }
  }

  /// Once the earliest question or grant in `shown` ends, takes it away here at once, its notification with it, waits
  /// for the next, and reads again; the next end is waited for whether that reading works or not.
  ///
  /// - Parameter shown: What shows now.
  private func waitForEnd(of shown: AskList) {
    wake?.cancel()
    wake = nil
    // What ended already shows nowhere; waiting for it again would only read the same again.
    let now = wallClock()
    let ends = (shown.asks.map(\.ends) + shown.allowed.map(\.ends))
      .filter { $0 > now }
    guard let earliest = ends.min() else { return }
    let seconds = earliest.timeIntervalSince(now)
    wake = Task { [weak self, clock] in
      do {
        try await clock.sleep(for: .seconds(seconds))
      } catch {
        return
      }
      self?.expire()
    }
  }

  /// Takes away every question and grant that has ended, tells the notifications, waits for the next end and reads
  /// again, which the service answers with the ended ones taken down.
  private func expire() {
    // This wait is over; waiting for the next must not cancel the reading that follows.
    wake = nil
    let now = wallClock()
    if var shown = list {
      shown.asks.removeAll { $0.ends <= now }
      shown.allowed.removeAll { $0.ends <= now }
      list = shown
      keepPictures()
      changed(shown)
      waitForEnd(of: shown)
    }
    askRead()
  }
}
