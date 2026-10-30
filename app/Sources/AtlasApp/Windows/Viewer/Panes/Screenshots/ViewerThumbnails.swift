import AtlasService
import AtlasUI
import CoreGraphics
import Foundation

/// The bounded thumbnail queue, including holds, resume and cancellation.
final class ViewerThumbnails {
  /// The panes that receive completed work; never keeps the window alive.
  private weak var model: ViewerPanesModel?
  private let service: any ViewerService
  /// Two reads fill the grid without crowding out status and picture requests.
  static let atOnce = 2
  /// Pending and active thumbnail reads, with their cancellation generation.
  private(set) var work = ThumbnailWork()

  /// Creates the shared thumbnail queue for Activity and Screenshots.
  ///
  /// - Parameters:
  ///   - model: The panes whose cache receives each thumbnail.
  ///   - service: The thumbnail service.
  init(model: ViewerPanesModel, service: any ViewerService) {
    self.model = model
    self.service = service
  }

  /// Follows pane visibility without changing the cache or losing pending requests.
  func follow(shown: Bool, place: ViewerPlace) {
    guard let model else { return }
    if !shown || (place != .activity && place != .screenshots) {
      resetThumbnails()
    } else {
      work.wanted = work.wanted.filter { name in
        model.panes.shots.shots?.contains(where: { $0.name == name }) == true && model.panes.thumbnails[name] == nil
      }
      readThumbnails()
    }
  }

  /// A thumbnail came into view: read it, unless it is kept, being read or waiting already.
  ///
  /// - Parameter name: The screenshot's file name.
  func thumbnailNeeded(_ name: String) {
    guard let model else { return }
    guard model.panes.thumbnails[name] == nil, work.loading[name] == nil,
      !work.wanted.contains(name)
    else { return }
    work.wanted.append(name)
    readThumbnails()
  }

  /// Holds new thumbnail reads while the window is being resized, so tiles coming into view as it changes wait in turn
  /// rather than read in a burst; once it ends, those waiting are read.
  ///
  /// - Parameter held: Whether new reads wait.
  func holdThumbnails(_ held: Bool) {
    guard work.isHeld != held else { return }
    work.isHeld = held
    if !held { readThumbnails() }
  }

  /// Starts the waiting thumbnails, oldest first, while fewer than ``atOnce`` are being read, unless reads
  /// are held.
  private func readThumbnails() {
    while !work.isHeld, work.loading.count < Self.atOnce,
      !work.wanted.isEmpty
    {
      let name = work.wanted.removeFirst()
      let number = work.generation
      work.loading[name] = Task { [weak self, service] in
        var image: CGImage?
        // A thumbnail that cannot be read lands as none, its tile left as it was.
        if let jpeg = try? await service.screenshot(name: name, full: false) {
          image = await ImageDecoding.image(jpeg)
        }
        self?.land(image, for: name, number: number)
      }
    }
  }

  /// A thumbnail read: kept, unless the queue was let go since it was asked for, and the next one starts.
  ///
  /// - Parameters:
  ///   - image: The thumbnail, or nil when it could not be read.
  ///   - name: The screenshot's file name.
  ///   - number: The queue's generation when it was asked for.
  func land(_ image: CGImage?, for name: String, number: Int) {
    guard let model else { return }
    guard number == work.generation else { return }
    work.loading[name] = nil
    if let image { model.panes.thumbnails.keep(DesktopPicture(id: name, image: image), for: name) }
    readThumbnails()
  }

  /// Pauses the queue: what is being read is cancelled and waits with the rest for the pane to show again. Any reply
  /// still to come is dropped. What is kept stays.
  func resetThumbnails() {
    work.generation += 1
    work.wanted.insert(contentsOf: work.loading.keys, at: 0)
    for load in work.loading.values { load.cancel() }
    work.loading.removeAll()
  }
}
