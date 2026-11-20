import AtlasService
import Observation

/// The desktops the service serves and the one Atlas shows, as every window lists them: read with the panel's status,
/// which the service's feed wakes when they change, and again once a desktop is picked. Each move of the desktop shown
/// is counted, so the windows forget what they showed of the one before (``moves``). ``HelperActions`` holds the one
/// every window shares.
@Observable
final class DesktopStore {
  /// The desktops as `desktops.json` last said; nil before it was read, and from an older service that lists none.
  private(set) var list: DesktopList?
  /// How many times the desktop shown moved since the list was first read, whichever window or command moved it.
  private(set) var moves = 0
  /// The viewer service it reads.
  @ObservationIgnored private let service: any ViewerService

  /// A store that has read nothing yet.
  ///
  /// - Parameter service: The viewer service it reads.
  init(service: any ViewerService) {
    self.service = service
  }

  /// Reads the desktops again. A list that can't be read now keeps the one known; an older service that lists none
  /// leaves none, as one desktop.
  func read() async {
    let read: DesktopList
    do {
      read = try await service.desktops()
    } catch {
      if error.isNotOffered, list != nil { list = nil }
      return
    }
    if let shown = list?.viewed, shown != read.viewed { moves += 1 }
    if list != read { list = read }
  }

  /// Whether more than one desktop is set up, so the windows offer to pick the one shown.
  var picks: Bool { (list?.desktops.count ?? 0) > 1 }

  /// The desktop shown, as the list names it; nil before it was read, or while it names none.
  var viewed: DesktopList.Entry? {
    guard let list else { return nil }
    return list.desktops.first { $0.id == list.viewed } ?? list.desktops.first(where: \.isViewed)
  }

  /// Each desktop's title by its name in the settings, as a Stop names them.
  var titles: [String: String] {
    let pairs = (list?.desktops ?? [])
      .map { ($0.name, Words.title(of: $0)) }
    return Dictionary(pairs) { first, _ in first }
  }
}
