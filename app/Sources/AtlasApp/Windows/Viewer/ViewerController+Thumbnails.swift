import AtlasUI
import CoreGraphics

/// The screenshots' thumbnails, which Activity's screenshot lines and the Screenshots grid share: read through one
/// queue a couple at a time as their tiles come into view, decoded off the main actor, kept by name within a bound,
/// and dropped when they come back after the queue was let go.
extension ViewerController {
  /// 2: thumbnails read at once. Each is a JPEG the service makes from a PNG of several megabytes, so two keep the
  /// grid filling steadily without crowding out the status and picture readings, which share the service's few
  /// request slots.
  static let thumbnailsAtOnce = 2

  /// A thumbnail came into view: read it, unless it is kept, being read or waiting already.
  ///
  /// - Parameter name: The screenshot's file name.
  func thumbnailNeeded(_ name: String) {
    guard panes.thumbnails[name] == nil, paneWork.thumbnails.loading[name] == nil,
      !paneWork.thumbnails.wanted.contains(name)
    else { return }
    paneWork.thumbnails.wanted.append(name)
    readThumbnails()
  }

  /// Starts the waiting thumbnails, oldest first, while fewer than ``thumbnailsAtOnce`` are being read.
  private func readThumbnails() {
    while paneWork.thumbnails.loading.count < Self.thumbnailsAtOnce, !paneWork.thumbnails.wanted.isEmpty {
      let name = paneWork.thumbnails.wanted.removeFirst()
      let number = paneWork.thumbnails.generation
      paneWork.thumbnails.loading[name] = Task { [weak self, service] in
        var image: CGImage?
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
    guard number == paneWork.thumbnails.generation else { return }
    paneWork.thumbnails.loading[name] = nil
    if let image { panes.thumbnails.keep(DesktopPicture(id: name, image: image), for: name) }
    readThumbnails()
  }

  /// Lets the queue go: what waits is forgotten, what is being read is cancelled, and any reply still to come is
  /// dropped. What is kept stays.
  func resetThumbnails() {
    paneWork.thumbnails.generation += 1
    paneWork.thumbnails.wanted.removeAll()
    for load in paneWork.thumbnails.loading.values { load.cancel() }
    paneWork.thumbnails.loading.removeAll()
  }
}
